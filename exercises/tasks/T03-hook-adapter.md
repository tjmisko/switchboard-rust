# T03 — JSON navigation and the hook adapter

**Core: 2h** · Gaps: **Option/Result type flow** (the main event), `and_then`
vs `map`, `filter`, `zip`, `try_fold`, `Option` as an iterator, lifetimes on
borrowed returns · Test: `cargo test --test t03_hook`

This is the largest combinator workout in the course. Everything a hook says
arrives as a `serde_json::Value`, and every fact is pulled out as
`Option<&Value> → Option<&str> → Option<Domain>`, turning into a `Result` only
where the protocol demands it.

It is two halves of about an hour each. The first is pure combinators that
the tests specify completely; the second is a domain mapping. Do them in
order, and check the first before starting the second.

## Half one: the combinators

**`src/ingest/json.rs`**: `str_at`, `nonblank_str_at`, `u64_at`, `bool_at`,
`at` (**use `try_fold`**), `str_path` and `array_at`. `array_at` returns
`impl Iterator`, and gets there without a branch: `Option<&Vec<Value>>` is
itself iterable.

**`src/ingest/hash.rs`**:

- `fnv1a(bytes: &[u8]) -> u64`, as one `fold`.
- `input_hash(input: &Value) -> InputHash`. Read the module comment on
  canonical form first.
- `InputHash::to_hex` and `InputHash::from_hex`. The test has two traps in
  it. One is `+f00…`: read the docs for `u8::from_str_radix` closely. The
  other is `ééééééé00`, which is sixteen *bytes* long, so a length check on
  its own lets it through.

**`src/ingest/forward.rs`**: `scrub(raw: Value, argv_event: Option<&str>, pid: u32) -> Value`.
It takes `raw` **by value**. `Map::remove` hands back the owned `tool_input`,
so you can hash it without cloning.

**Checkpoint.** These ten tests cover everything above and nothing below:

```
cargo test --test t03_hook -- should_read should_walk should_iterate should_return should_hash should_round should_drop should_fall should_still
```

## Half two: the adapter

**`src/ingest/claude/hook.rs`**: `hook(frame: &HookFrame, ctx: &Ctx) -> Vec<Input>`
and its helpers (`actor`, `prompt_key`, `prompt_origin`, `call_and_tool`,
`frame_hash`, `waits` and `wait_kind`). The mapping table is in the module
comment. Read all of it before writing a line, including the actor exception
for `SubagentStart`/`SubagentStop` and the two documented fields that no
captured payload carries: `session_title` and `UserPromptSubmit.source`. A
prompt with no `source` has an `Unknown` origin, not a human one.

For which fields each event carries, the reference is the table in
`docs/product/survey/14-hook-events-verified.md` §1.1. Nothing else in
`docs/` is needed for this task.

### Decisions the tests do not make (now they do)

Eight places where a payload can leave the adapter without an answer. Each
is what golden does, and each has a test in the last section of `t03_hook`:

| Case | Golden's answer |
|---|---|
| `SubagentStop` has no outcome field | `SubagentOutcome::Completed`. The transcript tells the failed and killed stories |
| `PermissionDenied.reason` | Never read. It is an open string shared with other subsystems, not the `toolDenialKind` set, so the call settles as `Denied { by: PermissionRule }` even when `reason` happens to spell a `Denier`. `call` is the `tool_use_id`, `req` is `None` |
| `Elicitation` has no `tool_name` | The gate is named after `mcp_server_name`, else `mcp`. `call: None`, `hash: None`, `GateKind::Question` |
| `SessionEnd` without `reason` | The session still ends; the reason reads `other` |
| `SessionStart` with an unrecognised or absent `source` | `StartSource::Startup`, `Cause::Agent`. The session did start |
| `agent_type` to `SubagentKind` | Not read. Every hook-announced child is `Task` |
| Which `background_tasks[].status` values are live | `running`, `pending`, or no `status` at all. Anything else is finished and is dropped |
| `PreToolUse` with no `tool_input` and no hash | Still a `ToolDispatched`; its hash is `input_hash(&Value::Null)`. Only `tool_use_id` and `tool_name` are required |

And one that is not a gap but was untested: `background_tasks: []` with no
crons still yields a `Waits` snapshot, with no rows. `Some(vec![])` and
`None` are different answers and the reducer needs both.

## Constraints that make it a workout

- **No `unwrap`, `expect` or `match` on an `Option` in `json.rs` or
  `hash.rs`.** Only combinators. For `from_hex` that means `bool::then`
  (a `bool` into an `Option`) or `Option::filter`, plus `flatten`; both
  are the branch you are not allowed to write.
- In `hook.rs`, a missing required field becomes `let ... else { return
  Vec::new() }`. Don't nest `if let`s.
- `call_and_tool` is a single `zip`.
- `frame_hash`: prefer the forwarder's `tool_input_hash`, and fall back to
  hashing a raw `tool_input`. That's one chain with `or_else`.
- `waits`: `filter`, `filter_map`, `chain`, `collect`. Note the difference
  between "neither key is present" (`None`) and "present and empty"
  (`Some(vec![])`). The reducer treats those two very differently.
- `wait_kind`: the captured `type` vocabulary is `shell`, `monitor` and
  `workflow`. Anything else is `Other`: carried, not guessed.

## Katas

1. Write the type after each call:
   ```rust
   p.get("agent_id")            // ?
    .and_then(Value::as_str)    // ?
    .and_then(SubagentId::from_hook)  // ?
    .map_or(Actor::Agent, Actor::Subagent)  // ?
   ```
   Why is `Actor::Subagent` usable as a function there?
2. Why is this a compile error, and what two signatures would fix it?
   ```rust
   fn session_of(line: &str) -> Option<&str> {
       let v: Value = serde_json::from_str(line).ok()?;
       v.get("session_id").and_then(Value::as_str)
   }
   ```
3. `str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str>` has no lifetime on
   `key`. What would change if you wrote `key: &'a str`? The test
   `should_return_a_str_that_borrows_from_the_value_not_from_the_key` is the
   answer.

## Done when

`t03_hook` passes: 40 tests covering helpers, hash, scrub, the adapter end
to end from raw fixture payloads, and the eight decisions above. The
fixtures under `tests/fixtures/claude/hooks/` are captured shapes from Claude
Code 2.1.269/2.1.270, September 2026, with three exceptions built from the
documented field sets because no capture exists: `permission_denied.json`,
`elicitation.json` and `session_end.json`. The `session_start_clear.json`
title and `user_prompt_submit.json` source are likewise the documented shape,
kept so the adapter tolerates a build that sends them.
