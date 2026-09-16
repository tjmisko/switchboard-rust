# Module layout and the two axes of churn

Two things change underneath this daemon at different rates and for unrelated
reasons:

- **Platforms.** Linux/Hyprland/wezterm today; macOS is required; Windows is
  possible. Changes rarely, in large steps, and is known at compile time.
- **Harnesses.** Claude Code and Codex ship most days. Hooks appear, fields
  appear, an event starts firing in a case it did not. Changes constantly, in
  small steps, and is only knowable at runtime.

Neither may reach the reducer. They need different mechanisms, because one is
a compile-time choice and the other is a runtime discovery.

## 1. Tree

```
src/
  model/          types only. No I/O, no provider names, no cfg attributes.
    mod.rs        re-exports
    ids.rs        newtypes, ProcKey, keys
    event.rs      Event, EventKind, Cause, outcomes
    snapshot.rs   Snapshot, SnapshotKind, AsOf
    agent.rs      Agent, Activity, Tool, Gate, Prompts, Wait
    subagent.rs   Subagent, Subagents
    phase.rs      Phase, Severity, Shown, Confidence   <- the cost model
    board.rs      Board

  reduce/         pure transitions over model. No I/O.
    event.rs      apply_event
    snapshot.rs   apply_snapshot
    tick.rs       expiry, dwell promotion, staleness
    effect.rs     Effect

  ingest/         bytes -> model::Event | model::Snapshot
    claude/       hooks.rs transcript.rs statusline.rs panel.rs
    codex/        hooks.rs appserver.rs
    capability.rs what this installation can actually tell us

  platform/       traits + one impl per OS
    mod.rs        the traits
    linux/        procfs, pidfd, hyprland, sway, x11
    macos/        libproc, kqueue, accessibility/CGWindowList
    windows/      toolhelp32, job objects, win32

  surface/        terminal multiplexers: wezterm, tmux, kitty
  publish/        state.json, rpc, history, and per-OS indicators
  daemon/         wiring, tasks, the effect runner
```

Two rules with teeth:

- `model/` and `reduce/` contain no `#[cfg(target_os)]` and no string literal
  naming a provider. Worth a test that greps for both; it is the kind of rule
  that erodes silently.
- `ingest/` may not hold state. Adapters are `fn(Raw) -> Option<Input>`. All
  state lives in `Board`, so a provider quirk cannot grow a private cache that
  disagrees with the reducer.

## 2. Platform: narrow behavioural traits

The boundary is not "an OS abstraction". It is the four things the daemon
actually needs, expressed so each OS can satisfy them its own way.

```rust
pub trait Processes {
    /// Every agent-looking process, with identity and CPU time.
    fn sweep(&self) -> Vec<ProcessRow>;
}

pub trait DeathWatch {
    /// Exact notification, no polling, no pid-reuse race.
    /// Linux pidfd · macOS kqueue EVFILT_PROC/NOTE_EXIT · Windows job object.
    fn watch(&self, key: ProcKey) -> BoxFuture<'static, ()>;
}

pub trait Locator {
    /// Where each of these agents is on screen. The platform owns the join.
    fn locate(&self, of: &[ProcKey]) -> Vec<PlacementRow>;
}

pub trait Activator {
    fn focus(&self, at: &Placement) -> io::Result<()>;
}
```

`Locator` taking processes and returning `PlacementRow { process, placement }`
is the load-bearing choice. An earlier draft keyed placement rows by `tty`,
which reads fine on Unix and is meaningless on Windows — it put a `dev_t` in
the middle of a portable model to serve one platform's implementation detail.
Letting the platform own the join keeps the controlling-terminal lookup, the
`wezterm cli list` call and the window-manager query entirely inside
`platform::linux`, and hands the model a conclusion.

`ProcKey { pid, start }` does generalise: `proc_bsdinfo.pbi_start_tvsec` on
macOS, `GetProcessTimes` creation time on Windows. So the identity story is
portable even though every method of obtaining it differs.

What does *not* generalise, and should be admitted rather than abstracted:
Waybar is Linux-only, so `publish/` gets a per-OS indicator module rather than
one interface pretending a macOS menu bar extra and a Waybar slot file are the
same thing.

## 3. Harnesses: capability as runtime data

The wrong shape is `if provider == Claude && version >= (2, 1, 268)`. Version
comparisons accumulate, are never deleted, and are wrong the moment a user
runs a build you did not anticipate.

The right shape: **a capability is something this installation can actually
tell us, discovered at startup.**

```rust
pub struct Capabilities {
    pub turn_id: bool,            // Claude prompt_id, Codex turn_id
    pub permission_call_id: bool, // false on Claude: no tool_use_id
    pub tool_failure: bool,       // PostToolUseFailure
    pub denial_reason: bool,      // PermissionDenied
    pub turn_failure: bool,       // StopFailure, or app-server TurnError
    pub interrupt_event: bool,    // Codex Interrupt
    pub child_panel: bool,        // subagentStatusLine
    pub limits_feed: bool,        // statusLine, or account/rateLimits/updated
    pub gate_ownership: bool,     // approvalsReviewer
}
```

Crucially, a capability is **emitted ∧ subscribed**. An event the binary emits
but that nothing is wired to deliver is not a capability. So detection has two
halves:

- *Emitted*: probe the installation. Codex is self-describing —
  `codex app-server generate-json-schema` prints the whole protocol. Claude's
  bundle can be searched for emitter objects, which is how the event census in
  `survey/14` was produced in the first place. Both are scriptable.
- *Subscribed*: read `~/.claude/settings.json` and `~/.codex/hooks.json` and
  see what is actually wired to us.

Then policy is a function of capabilities, never of provider and version:

| Capability | Present | Absent |
|---|---|---|
| `permission_call_id` | join a prompt to its call by id | join on `(actor, tool, input hash)`, keep `Prompts::unjoined` |
| `tool_failure` | exact release on interrupt during a tool | fall back to the transcript |
| `turn_failure` | `Blocked` on a real signal | infer from a transcript error line, at `Confidence::Inferred` |
| `child_panel` | subagents are level-triggered, no expiry needed | `subagents/` scan plus the stale cap |
| `gate_ownership` | `Ownership::Human`, render `Attention` | `Ownership::Unknown`, render `Unknown` after the grace |
| `limits_feed` | `Blocked` with a real `resets_at`. The 2.1.269 status line reports `rate_limits.{five_hour, seven_day}.{used_percentage, resets_at}` and no status; the fullest window decides, at snapshot confidence | tail-scan the transcript |

Every row is a fallback that already exists. A new harness release flips a flag
and the fallback stops being used. No code changes, and — more to the point —
no code deleted, so a user on an older build keeps working.

### 3.1 Forward compatibility is not optional at this release cadence

- Unknown hook events parse to `EventKind::Unsupported { name }`, get counted,
  and are ignored. Never a parse failure, never a crash.
- Unknown JSON fields are ignored; unknown enum values map to an `Other`
  variant. A new `toolDenialKind` must not take the daemon down.
- Every adapter is tested against captured payloads in `tests/fixtures/`, so a
  harness change that alters a shape fails a named test rather than silently
  changing behaviour.

### 3.2 Detecting the change, rather than being told about it

`switchboard probe` re-runs the introspection and writes a manifest:

```toml
# capabilities/claude-2.1.268.toml     generated, checked in
[emitted]
PreToolUse          = ["tool_name", "tool_input", "tool_use_id"]
PermissionRequest   = ["tool_name", "tool_input", "permission_suggestions"]
PostToolUseFailure  = ["tool_use_id", "error", "is_interrupt", "duration_ms"]

[derived]
permission_call_id = false
turn_id            = true
```

Diffing the manifest after an upgrade says exactly what changed: an event that
appeared, a field that vanished, an enum that grew. That turns a daily-moving
target into a reviewable diff, and it is the only part of this design that
scales with the harnesses' release cadence rather than against it.

The counts in the manifest are also the honest answer to "is this still true?"
for every claim in `survey/14`.

## 4. What stays in one place

- The cost model: `model/phase.rs`, in `Shown::settle`. One function.
- Fold order: `Agent::phase`. One function.
- Provider naming: `ingest/`. Nowhere else.
- OS differences: `platform/` and `publish/`. Nowhere else.

If any of those four appears in a second place, that is the defect, regardless
of whether it is currently producing a wrong answer.
