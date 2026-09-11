# 04 — The Navigate tier: WM, terminal, session→window mapping, focus/cycle

Survey of the Go implementation under `~/Projects/switchboard` (worktrees
ignored). Paths below are relative to that root. Line numbers are from the
tree as of 2026-09-10.

## 1. Responsibilities

- **Enumerate OS windows** from whichever window manager is running (Hyprland,
  sway/i3, any EWMH X11 WM) as a neutral `wm.Window{Address, PID, Title,
  Workspace, WorkspaceID}` and stream neutral focus/close/layout events.
- **Locate the terminal pane** that owns an agent's controlling tty
  (wezterm, tmux, or a chain of both) as a neutral `terminal.PaneRef`, and
  activate that pane on demand.
- **Join session → pane → window.** tty is the portable join key between
  process and pane; `(mux pid, window title)` or the `[sbw:<gui-pid>:<window-id>]`
  title marker is the join between pane and WM window.
- **Keep the join fresh** on a reconcile tick and on WM events, batching one
  terminal enumeration + one WM enumeration per tick rather than per session.
- **Track the focused session** from the WM's active window and publish a
  window-level `Focused` bit.
- **Execute focus** for local sessions (WM raise + pane activate) and for
  remote federated sessions (pane-bind registry → wezterm activate → WM raise).
- **Provide the navigation policy in `switchboard-ctl`**: selector resolution
  (`pid:`/`idx:`/`host:`), unrestricted `cycle next|prev`, and the bounded
  urgency-first `attention` ring.
- **Detect backends at runtime** and publish a `capabilities` block
  (`navigate` = terminal != none AND wm != none).
- **Normalize titles** (spinner glyph stripping) so animated agent titles do not
  break the title-based join, and **label sessions** for display (`internal/label`).
- **Announce pane bindings** by writing an OSC 1337 `SetUserVar` escape into
  the session's tty so the WezTerm Lua integration can report pane→session.

## 2. Core types

### Domain (what a Rust rewrite must preserve)

| Type | Where | Notes |
|---|---|---|
| `wm.Window` | `internal/wm/wm.go:24-30` | Address is opaque, backend-owned (Hyprland `0x…`, i3 `con_id` decimal, X11 window id decimal). |
| `wm.Event{Kind, Address}` / `EventKind` | `wm.go:34-52` | Three kinds: `focus-changed` (address = new active, "" if none), `window-closed` (address), `layout-changed` (no address). |
| `wm.Manager` | `wm.go:59-74` | `Name() Available() Clients(ctx) ActiveWindow(ctx) Focus(ctx, ref) Subscribe(ctx) <-chan Event`. |
| `wm.ErrUnsupported` | `wm.go:56` | Returned by `none.Focus`. |
| `terminal.PaneRef` | `internal/terminal/terminal.go:24-36` | `Backend, Handle, Mux, MuxSocket, PaneID, TabID, WindowID, Title, WindowTitle, TTY, CWD`. `(Mux, PaneID)` is stable identity for wezterm; `Handle` (`%3`) for tmux. `Mux == 0` means "no WM join possible". |
| `terminal.Locator` | `terminal.go:43-53` | `Name() Available() Locate(ctx, tty) (*PaneRef, error) Activate(ctx, *PaneRef) error`. `(nil, nil)` = tty not owned. |
| `terminal.Snapshotter` (optional) | `terminal.go:65-75` | `Snapshot(ctx) map[tty]PaneRef`. Non-nil empty map = "owns nothing" (success); error = incomplete. |
| `state.HyprlandInfo` | `internal/state/state.go:178-183` | `Address, Workspace, WorkspaceID, Monitor` (Monitor never populated, decisions #6). Name is a legacy misnomer; it is the neutral window block on the wire. |
| `state.WeztermInfo` | `state.go:155-176` | `MuxPID, MuxSocket, PaneID, TabID, WindowID, WindowTitle, Title (json:"-"), TitleAt`. |
| `state.Session.Focused/Headless/Remote/Navigable` | `state.go:31-50` | `Focused` is window-level. `Navigable` only on aggregate (federated) rows. |
| `state.Capabilities` | `state.go:687-692` | `{observe, navigate, wm, terminal}`. |
| `panebind.ExactSessionKey` | `internal/panebind/types.go:45-49` | `(Hostname, PID, StartedAt)`; `StartedAt` is a stale-action fence, not a kernel birth token. |
| `panebind.LocalPaneRef` | `types.go:102-106` | `(GUIPID, WindowID, PaneID)`; `paneIdentity` = `(GUIPID, PaneID)` because a pane keeps its id across window moves (`registry.go:20-23`). |
| `panebind.WindowMarker` | `types.go:117-119` | `[sbw:<gui-pid>:<window-id>]`, the exact join between a WezTerm window and a WM client. |
| Mapping result | `mapping.go:57-95` (Resolve) | A `state.Session` with `Wezterm` and `Hyprland` pointers filled or left nil. There is no dedicated result type. |

### Plumbing (Go-specific, can be reshaped)

- `terminal.auto` (`auto.go:18-79`) re-detects on every call; `terminal.chain`
  (`chain.go:15-122`) composes locators innermost-first and routes `Activate`
  by `PaneRef.Backend`.
- `mapping.Resolver{term, wm, windows func([]wm.Window)}` (`mapping.go:27-31`)
  plus `Enumerate`/`ReconcileFrom` (`mapping.go:146-223`), which exist purely to
  hoist I/O out of the store lock.
- `resolveTurn` mutex (`cmd/switchboard/main.go:857-869`) serializing the two
  producers of the mapping.
- `wezterm.Pane` / `cliPane` (`internal/wezterm/wezterm.go:24-46`): wezterm's
  CLI JSON row plus `MuxPID/MuxSocket` tagging.
- `hyprland.Client` (`internal/hyprland/hyprland.go:27-40`): `hyprctl clients -j` row.
- `i3Node` (`internal/wm/i3.go:260-274`): GET_TREE node.
- `label.NameCache` (`internal/label/label.go:423-429`): stat-keyed memo of
  `~/.claude/sessions/<pid>.json` names.

## 3. Concurrency

### Goroutines, timers, mutexes, blocking I/O

| Item | Started by | Shutdown | Communicates |
|---|---|---|---|
| **WM loop** `runWMLoop` | `main.go:258` (`go runWMLoop`) | ctx cancel; re-subscribes after 2 s on error/EOF (`main.go:640-655`) | consumes `<-chan wm.Event`, writes to `state.Store` via `Apply`, records history focus events |
| **Layout debounce timer** in `drainWMEvents` | per subscription (`main.go:667-739`) | `defer timer.Stop()` | 200 ms leading-edge debounce (`layoutDebounce`, `main.go:638`); armed on first layout event of a burst, flushed before any non-layout event |
| **Reconciler** `runReconciler` | `main.go:259` | ctx cancel | `time.NewTicker(interval)` (`main.go:892`); calls `reconcileOnce` under `resolveTurn` |
| **`resolveTurn` mutex** | `main.go:253` | n/a | serializes reconcile tick vs layout re-resolve so an older enumeration never lands after a newer one (`main.go:857-869`). Must always be taken BEFORE `store.Apply`, never inside. |
| **Hyprland socket2 reader** | `hyprland.Subscribe` (`hyprland.go:182`) + a watcher goroutine (`:185`) that closes the conn on ctx.Done | conn close unblocks `bufio.Scanner` | raw `hyprland.Event{Name, Data}` into a 32-buffered chan |
| **Hyprland → neutral translator** | `wm.Hyprland.Subscribe` (`wm/hyprland.go:65`) | range over raw chan ends / ctx.Done on send | 32-buffered `chan wm.Event` |
| **i3 event reader** | `wm.I3.Subscribe` (`i3.go:120`) + conn-closer goroutine (`:122`) | `conn.Close()` on ctx.Done unblocks `io.ReadFull` | 32-buffered `chan wm.Event` |
| **X11 event reader** | `wm.X11.Subscribe` (`x11.go:144`) + conn-closer (`:147`) | `c.Close()` on ctx.Done; `WaitForEvent` returns nil,nil | 32-buffered `chan wm.Event` |
| **none WM Subscribe** | `wm/none.go:26-31` | goroutine waits on ctx.Done then closes chan | never emits |
| **Hyprland request socket** | `hyprland.request` (`hyprland.go:219-239`) | 2 s dial timeout, 2 s deadline or ctx deadline; one connection per command, read to EOF | blocking, synchronous |
| **i3 roundtrip** | `i3.roundtrip` (`i3.go:200-220`) | same 2 s pattern; fresh dial per command | blocking |
| **X11 property reads** | per-op `xgb.NewConn()` (`x11.go:35,75,97`) | conn closed per op; no timeout on xgb round trips | blocking |
| **`wezterm cli list --format json`** subprocess | `wezterm.listOne` (`wezterm.go:145-175`), once per live mux | `exec.CommandContext` with 2 s timeout | stdout JSON |
| **`wezterm cli activate-pane --pane-id N`** | `wezterm.ActivatePane` (`wezterm.go:196-206`) | 2 s timeout | CombinedOutput |
| **`tmux list-panes -a -F …`** / `select-window` / `select-pane` | `tmuxRun` (`tmux.go:157-171`) | 2 s timeout | stdout, "no server running" stderr sentinel |
| **gui-sock peer check** | `guiSocketPeerPID` (`socket_peer_linux.go:14-40`) | 250 ms dial | `SO_PEERCRED` |
| **OSC announce write** | `panebind.Announcer.Announce` (`osc.go:71-105`) | `O_NONBLOCK` fd, ≤ 8 write attempts (`osc.go:17`) | writes to `/dev/pts/N` |
| **`panebind.Registry.mu`** RWMutex | `registry.go:11` | n/a | route table |
| **`federation.WorkspaceIndex.mu`** RWMutex | `workspace.go:37` | n/a | cached WM enumeration |
| **`label.NameCache.mu`** Mutex | `label.go:424` | n/a | reads happen OUTSIDE the lock deliberately (`label.go:541-544`) |
| **Federation route loop** `RunLiveRoutesReady` | `cmd/switchboard/federation.go:135` | ctx cancel / source chan closed → `DropLiveHost` for every known host | `registry.ReplaceLive` per remote snapshot |
| **WezTerm Lua callbacks** (out of process) | `update-status`, `user-var-changed`, `window-focus-changed` (`switchboard.lua:246-316`) | WezTerm lifetime | spawn `switchboard-ctl pane-bind` / `pane-state` synchronously via `run_child_process`, serialized per pane by a generation lane |

Every WM `Subscribe` follows one shape: dial, spawn reader goroutine, spawn
a tiny closer goroutine on `ctx.Done`, `defer close(out)`. Reconnection is
owned by `runWMLoop`, not the backend.

### Diagram

```
                     ┌──────────────────────┐
   Hyprland socket2  │  wm backend goroutine │  chan wm.Event (32)
   i3 IPC conn ─────▶│  (reader + closer)    │────────────┐
   X11 root PropNfy  └──────────────────────┘             ▼
                                                 ┌──────────────────────┐
                                                 │ runWMLoop /          │  focus/close: store.Apply
                                                 │ drainWMEvents        │──▶ applyFocus / endSession
                                                 │  [200ms layout timer]│
                                                 └──────────┬───────────┘
                                                            │ layout burst → reresolveAll
                                                            ▼
      ┌───────────────────┐   turn.Do   ┌────────────────────────────────┐
      │ runReconciler     │────────────▶│ resolveTurn (mutex)            │
      │ ticker(interval)  │             │  Enumerate() OUTSIDE store lock│
      └───────────────────┘             │   ├ terminal.Snapshot (fork    │
                                        │   │  wezterm cli list ×muxes,  │
                                        │   │  tmux list-panes)          │
                                        │   └ wm.Clients (socket/xgb)    │
                                        │  ReconcileFrom per session     │
                                        │  store.Apply (copy pointers)   │
                                        └────────────────────────────────┘
                                                  │ SetWindowObserver
                                                  ▼
                                        federation.WorkspaceIndex (cached clients)

   switchboard-ctl focus/cycle/attention ──RPC "focus-session"──▶ rpc.Server.exactFocus
        = federation.Navigator.Focus ─ local ──▶ rpc.FocusLocalSession ─▶ wm.Focus + term.Locate/Activate
                                     └ remote ─▶ Registry.Resolve → LocalResolver.Resolve
                                                 → wezterm.ActivatePane → Revalidate → wm.Focus
   WezTerm Lua ──switchboard-ctl pane-bind/pane-state──▶ Navigator.BindPane / PaneState
   daemon ──OSC 1337 SetUserVar SWITCHBOARD_SESSION──▶ tty ──▶ WezTerm user-var-changed
```

## 4. External boundaries

### 4.1 Detection order

`internal/detect/detect.go:77-91` (WM) and `:93-109` (terminal). Probing is
env + stat only; nothing is spawned.

| Order | Backend | Signal | Source |
|---|---|---|---|
| 1 | `hyprland` | `wm.NewHyprland().Available()` → `$HYPRLAND_INSTANCE_SIGNATURE`, else live lock file under `$XDG_RUNTIME_DIR/hypr/<sig>/hyprland.lock` | `hyprland.go:269-330` |
| 2 | `sway` | `$SWAYSOCK` | `i3.go:34` |
| 3 | `i3` | `$I3SOCK` | `i3.go:37` |
| 4 | `x11` | `$DISPLAY` | `x11.go:32` |
| 5 | `none` | — | |
| Terminal (auto) | `tmux` then `wezterm`, composed | `tmux.Available`: `$TMUX` set OR `($TMUX_TMPDIR\|/tmp)/tmux-<uid>/default` exists (`tmux.go:35-46`); `wezterm.Available`: any live `gui-sock-<pid>` (`terminal/wezterm.go:22-25`) | `auto.go:25-48` |

The daemon's `-wm` and `-terminal` flags force one backend. Terminal detection
is NOT one-shot: `auto` re-probes on every call because the daemon wins the
login boot race against the terminal emulator (`auto.go:8-16`). `Capabilities`
is re-published every reconcile tick for the same reason (`main.go:918`).

### 4.2 Hyprland

- **Sockets**: `$XDG_RUNTIME_DIR/hypr/<sig>/.socket.sock` (request) and
  `.socket2.sock` (events) (`hyprland.go:241-254`).
- **Instance discovery** when env is missing: read each `<sig>/hyprland.lock`
  line-1 PID, `kill(pid, 0)` liveness (EPERM counts as alive), tie-break by
  newest `.socket.sock` mtime, fall back to newest socket with no live lock
  (`hyprland.go:294-359`).
- **Request wire**: write command text, read to EOF, one connection per
  command. Commands used: `j/clients`, `j/activewindow`, `j/getoption
  cursor:no_warps`, `/dispatch <lua>`, and a `[[BATCH]]` (`hyprland.go:44-161`).
- **Focus** is Lua-dialect only (Hyprland's Lua config parser rejects legacy
  `keyword`): `[[BATCH]]eval hl.config({cursor={no_warps=true}}) ; dispatch
  hl.dsp.focus({window="address:0x…"}) ; eval hl.config({cursor={no_warps=<prev>}})`
  (`hyprland.go:141-146`). Separator is ` ; ` so chunks may not contain `;`.
  Batch reply must be `ok` per line. If `getoption` fails, falls back to a
  plain dispatch (cursor may warp).
- **Event wire**: newline-delimited `name>>data`, split on first `>>`, 1 MiB
  scanner cap (`hyprland.go:199-214`). Consumed names and their neutral kinds
  (`wm/hyprland.go:88-94`): `closewindow`→closed, `activewindowv2`→focus,
  `movewindowv2`/`windowtitlev2`/`openwindow`→layout.
- **Address normalization**: socket2 emits addresses WITHOUT `0x`; `clients`
  reports WITH. The backend prepends `0x` in `NormalizeEventAddress`
  (`wm/hyprland.go:100-105`). This is decisions.md #1, the "single most fragile
  cross-layer contract".

### 4.3 sway / i3 (one backend, `wm/i3.go`)

- Socket from `$SWAYSOCK` or `$I3SOCK`; `Name()` is `sway` or `i3` accordingly.
- Framing: `"i3-ipc"` magic + u32 length + u32 type in **native byte order**
  (`i3.go:179-256`). Types used: `RUN_COMMAND=0`, `SUBSCRIBE=2`, `GET_TREE=4`.
  Event flag is bit 31; event types `workspace=0`, `window=3`.
- Clients: recursive GET_TREE walk; a node is a window if `window != null` or
  `app_id != ""`; Address is `strconv(id)` (con_id); workspace from the nearest
  `type=="workspace"` ancestor (`i3.go:303-328`).
- **i3 PID hydration**: i3 omits `pid`, so when `Name=="i3"` and `$DISPLAY` is
  set, all X11 window ids are batched through `_NET_WM_PID` reads via xgb
  (`i3.go:62-68, 354-388`). Sway nodes already carry `pid` and are left alone.
- Focus: `[con_id=N] focus`, reply JSON `[{success, error}]` (`i3.go:80-98`).
- Subscribe: dedicated connection, `SUBSCRIBE ["window","workspace"]`; window
  `change` values `close`→closed, `focus`→focus, `new`/`move`/`title`→layout;
  any workspace event→layout (`i3.go:435-461`).

### 4.4 X11 / EWMH (`wm/x11.go`, pure-Go `jezek/xgb`)

- Atoms: `_NET_CLIENT_LIST`, `_NET_ACTIVE_WINDOW`, `_NET_WM_PID`,
  `_NET_WM_DESKTOP`, `_NET_WM_NAME` (+ legacy `WM_NAME` fallback)
  (`x11.go:235-262`).
- Clients: read `_NET_CLIENT_LIST` off root (up to 1<<20 words), then per
  window `_NET_WM_PID`, `_NET_WM_NAME`, `_NET_WM_DESKTOP` (+1 → 1-indexed
  workspace string; `0xFFFFFFFF` sticky → unresolved) (`x11.go:34-72, 297-304`).
- Focus: `_NET_ACTIVE_WINDOW` ClientMessage to root, format 32, data
  `[2 (pager), 0 (CurrentTime), 0, 0, 0]`, mask SubstructureNotify|Redirect
  (`x11.go:92-118`).
- Subscribe: `PropertyChange` on root; `_NET_ACTIVE_WINDOW`→focus (re-reads the
  property for the address), `_NET_CLIENT_LIST`→layout. **Never emits
  window-closed** (`x11.go:120-183`).
- Address parse accepts base 0 (decimal or `0x`) defensively (`x11.go:225-231`).
- Workspace names are lost (`_NET_DESKTOP_NAMES` not read); see i3-polybar-plan.

### 4.5 wezterm (`internal/wezterm`, `internal/terminal/wezterm.go`)

- Mux discovery: `$XDG_RUNTIME_DIR/wezterm/gui-sock-<pid>`, kept only when
  `/proc/<pid>` exists (`wezterm.go:52-81`). Note: `/proc` check is Linux-only
  even though the file is untagged.
- `WEZTERM_UNIX_SOCKET=<sock> wezterm cli list --format json` per mux
  (`wezterm.go:145-175`). `replaceEnv` guarantees exactly one value of the var
  (`:212-221`). Rows parsed: `window_id, tab_id, pane_id, title, window_title,
  cwd (file:// URL), tty_name, is_active`. `is_active` is parsed but never
  carried onto the session (latency report D5).
- Failure of one mux is logged to stderr and skipped; the union continues
  (`wezterm.go:96-103`).
- Activate: `WEZTERM_UNIX_SOCKET=<sock> wezterm cli activate-pane --pane-id N`.
- `ListGUI(guiPID)` (`wezterm.go:110-136`) enumerates exactly one socket and
  verifies the socket's `SO_PEERCRED` pid equals the filename pid;
  mismatch → `ErrGUISocketPeer` (definitive PID reuse evidence).
- CWD decoding: `file://host/path` → percent-unescaped path, trailing `/`
  trimmed, `""` for no path component (`terminal/wezterm.go:91-105`, decisions #8).
- `WindowTitle` is spinner-normalized at the seam; `Title` (pane's own) is left raw.

### 4.6 tmux (`internal/terminal/tmux.go`)

- `tmux list-panes -a -F "#{pane_tty}\t#{pane_id}\t#{pane_current_path}\t#{window_name}"`
  (`tmux.go:131`), parsed by tab split, rows with <4 fields dropped.
- Activate: `tmux select-window -t %N` then `tmux select-pane -t %N` (`tmux.go:114-128`).
- `noServer` sentinel: stderr contains `no server running` → owns nothing, not an error.
- `PaneRef.Mux` is left 0: tmux exposes the in-pane pid, not the hosting
  emulator's, so the WM hop is unresolvable. tmux sessions get pane focus but
  **no WM raise** (`tmux.go:21-24`, portability-plan 3.3 "deferred").

### 4.7 WezTerm Lua integration (`integrations/wezterm/switchboard.lua`)

- Title marker: `format_window_title(base, tab)` strips any existing
  `[sbw:N:N]` suffix and appends `[sbw:<GUI_PID>:<tab.window_id>]`
  (`switchboard.lua:116-134`). The user must compose it into their single
  `format-window-title` handler (README).
- On `user-var-changed` for `SWITCHBOARD_SESSION` with a valid v1 payload
  (strictly validated in Lua, `switchboard.lua:52-114`), it runs
  `switchboard-ctl pane-bind <json> <gui-pid> <window-id> <pane-id>`.
- On `update-status` it runs `switchboard-ctl pane-state <gui-pid> <window-id>
  <active-pane-id> <true|false>` deduped by `(pane, focused, bind-token)`;
  `window-focus-changed` only invalidates the dedupe (`:259-316`).
- Per-pane generation lanes serialize the two helpers across `run_child_process`
  yields and config reloads (`:185-225`); a `wezterm.GLOBAL` token survives
  reloads (`:18-40`).

### 4.8 OSC announce (daemon → tty)

`panebind.OSCSequence` (`osc.go:43-65`) emits
`ESC ] 1337 ; SetUserVar=SWITCHBOARD_SESSION= BEL` (clear) followed by the same
prefix + base64(canonical JSON) + BEL. Payload v1 is
`{"v":1,"host":…,"pid":…,"started_at":RFC3339Nano}` ≤ 512 bytes
(`payload.go:12-16, 38-53`). The tty is opened `O_WRONLY|O_NOCTTY|O_NONBLOCK|
O_CLOEXEC|O_NOFOLLOW` and fstat-checked to be a char device (`tty_unix.go:30-46`).
Triggered on session discovery (`cmd/switchboard/federation.go:180-193`) and on
`announce-bindings` RPC before a remote-stream subscribe (`:197-202`).

### 4.9 RPC surface touching this tier (`internal/rpc/rpc.go`)

| Cmd | Line | Handler |
|---|---|---|
| `focus` (selector) | `:390` | `Server.focus` → `pickSession` → `focusLocalTarget` (`:561-627`). Legacy; ctl no longer sends it. |
| `focus-session` (host, pid, started_at) | `:403` | `exactFocus` = `federation.Navigator.Focus` |
| `pane-bind` | `:415` | `Navigator.BindPane` |
| `pane-state` | `:427` | `Navigator.PaneState` |
| `announce-bindings` | `:439` | `federationRuntime.AnnounceAll` |

## 5. The mapping algorithm

Inputs: `osproc.Info{PID, CWD, TTY}` from discovery. Output: `Session.Wezterm`
and `Session.Hyprland` (window block).

**Step 0 — no tty, no mapping.** `Resolve` returns bare session if `info.TTY == ""`
(`mapping.go:64-66`). Headless `claude -p` runs never map.

**Step 1 — tty → pane.** `term.Locate(ctx, tty)` with a 3 s timeout
(`mapping.go:68-74`). Under `auto`, the live backend set is recomputed per call
(`auto.go:33-48`); with both tmux and wezterm live, `chain.Locate` tries tmux
first, then wezterm, returning the first owner (`chain.go:45-60`). Match is an
exact string compare of the pane's `tty_name` / `pane_tty` against the process
tty (`wezterm.go:185-189`, `tmux.go:61-66`). First matching pane wins; the
batch path preserves the same first-wins rule (`terminal/wezterm.go:52-58`,
`tmux.go:93-97`, `chain.go:102-107`).

**Step 2 — populate terminal block.** Only when `pane.Backend == "wezterm"` is
`sess.Wezterm` set (`mapping.go:75-77`); tmux panes carry no wire block. On
initial `Resolve` only, an empty process cwd is backfilled from the pane cwd
(`mapping.go:78-80`).

**Step 3 — pane → WM window.** Skipped when `pane.Mux == 0` (tmux). Otherwise
`matchUniqueClient(clients, muxPID, windowID, windowTitle)` (`mapping.go:258-272`):

1. **Marker path.** Build `[sbw:<mux>:<windowID>]`; select clients with
   `PID == mux` AND trimmed title has that suffix. If ≥1 candidate matched, the
   marker path decides: exactly one → that window; more than one → nil
   (ambiguous, retry next tick). It does NOT fall through to legacy on a
   duplicate marker (`TestMatchUniqueClientFailsClosedOnDuplicateWindowMarker`).
2. **Legacy path** (no marker matched at all, i.e. integration not installed):
   select clients with `PID == mux` AND `panetitle.Normalize(client.Title) ==
   panetitle.Normalize(windowTitle)`. Exactly one → window; else nil.

`panetitle.Normalize` (`panetitle.go:17-29`) strips leading spinner glyphs
(`◐◑◒◓⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏✳⠂⠐⠁⠈⠠⠄⡀⢀`) repeatedly plus whitespace; it is a fixed point.

**Step 4 — reconcile semantics.** `ReconcileFrom` (`mapping.go:198-223`) applies
one tick's enumerations to one session with no I/O:
- tty absent from `panes` → return WITHOUT touching the session. A miss never
  clears an established mapping.
- Window match nil (zero or ambiguous) → keep the prior `Hyprland` address
  ("retry next tick", decisions #4 PRESERVE).
- Window match found → overwrite address/workspace in place.

**Step 5 — batch vs per-session.** `Enumerate` (`mapping.go:146-159`) returns
`(panes, clients)`; `panes == nil` means "no batch path or it failed" and the
caller degrades to per-session `Reconcile` (real I/O per session). `nil` ≠
empty map: an empty map is a real "nobody owns any tty" answer. `chain.Snapshot`
refuses the whole batch if any member lacks a Snapshotter (`chain.go:86-94`).

**Ancestor walk** (`rpc.findTrackedAncestor`, `rpc.go:1407-1421`) is NOT part of
mapping; it resolves a hook's caller pid to the tracked agent pid by walking
`PPID` up to 20 levels, stopping at the first tracked pid and refusing to cross
another agent process. It is the hook-attribution join, not the window join.

**Federated (remote) mapping** is a separate, registry-driven path:
1. Remote daemon writes the OSC into its session's tty (which is an SSH pane
   on the local machine).
2. Local WezTerm Lua decodes it and reports `pane-bind` → `Registry.BindReplacing`
   (`registry.go:68-104`) stores `(host,pid,started_at) ↔ (gui,pane)` as a
   *candidate*.
3. `RunLiveRoutes` (`routes.go:27-95`) feeds each remote snapshot to
   `Registry.ReplaceLive`; a candidate becomes navigable only when its exact key
   appears; once seen live, omission prunes it (`registry.go:173-206`).
4. At focus time `RouteResolver.Resolve` (`local.go:126-142`): registry
   resolve (live + exactly one pane) → `wezterm.ListGUI(gui)` with peer-cred
   check → pane must match `(gui, pane)` and socket basename → WM client list
   must contain exactly one `PID==gui` window with the marker for the pane's
   *current* window id → `RefreshLiveRoute` + `IsLiveRoute` final guard.

**Where ambiguity arises and how it is resolved:**
- Two wezterm windows with identical titles and no marker → both sessions stay
  unresolved until a title diverges (this is why `drainWMEvents` flushes the
  layout debounce before a focus event, `main.go:696-712`).
- Duplicate marker (should not happen) → nil, retry.
- Same tty reported by two muxes → first wins, consistently in both paths.
- Two panes of one window → both sessions get the same address and both read
  `Focused` (D5 in the latency report).
- Remote session bound to two panes → `ErrSessionAmbiguous`, fail closed.
- Remote route peer-pid mismatch → definitive miss → `UnbindRoute` prunes only
  that exact candidate (`navigator.go:52-60`).

## 6. Focus / cycle semantics

### Focused bit
`applyFocus` (`main.go:1104-1125`): `Focused = activeAddr != "" &&
sess.Hyprland.Address == activeAddr`. Window-level only. Driven by every
`EventFocusChanged` and re-synced each reconcile tick from `manager.ActiveWindow`
(`main.go:919, 1035`). A focus change records a history `focus` event keyed by
agent session id. Remote rows get `Focused` only through `Navigator.PaneState`,
which requires the local pane route to resolve AND the WM active window to equal
that route's window (`navigator.go:194-255`).

### Navigable membership (`switchboard-ctl`)
`sessionNavigable` (`ctl/main.go:475-486`): not `Headless`, and either a
host-local row (no `Hostname`) or an aggregate row with `Navigable == true`.
`Navigable` on aggregate rows comes from `Navigator.RouteReady` (`navigator.go:65-74`):
local host → always; remote → WM present + available AND registry resolves.

### `cycle next|prev` (`ctl/main.go:434-473`)
Ring = navigable sessions in snapshot order (StartedAt asc, PID tie-break,
decisions #2). Index = first `Focused` member. `next` → `(i+1) mod n`, or
`ring[0]` if none focused; `prev` → `(i-1) mod n`, or last. Unrestricted:
grey and every color included. Bound to scroll on waybar chips and polybar.

### `attention` (`ctl/main.go:497-598`, `docs/attention-ring-plan.md`)
Tiers: `permission`=0 (red), `idle`=1 (orange), `working|delegating`=2 (green),
anything else 3 (grey, excluded). Ring is built from the whole snapshot:
red present → red+orange; else orange present → orange+green; else green.
Selection:
1. If no urgent-tier member is focused → first urgent member.
2. Else the next un-focused urgent member after the focused one, in ring order.
3. Else (every urgent member focused) → first un-focused member of the adjacent tier.
4. Else nil (no-op). Red+green with no orange is a deliberate no-op when red is focused.

*Every* focused session is skipped, not just the origin, so a split window's
sibling pane is not chosen (the WM-level `Focused` compensation).

### Selectors (`resolveFocusSelector`, `ctl/main.go:233-304`)
`""`/`active` → focused else first; `idx:N`; `host:H:pid:N[:started:T]`;
`pid:N` (must be unique across hosts); bare `H:N`; bare number → PID first,
then index (decisions #3 back-compat). Every ctl focus resolves to an exact
`(Hostname, PID, StartedAt)` and sends `focus-session` (`ctl/main.go:215-231`).

### Local focus execution (`rpc.focusLocalTarget`, `rpc.go:596-627`)
Guard: both backends `none` → `ErrNavigateUnsupported`; headless →
`ErrHeadlessSession`. Then: if a window address exists, `wm.Focus` (error is
fatal); if a tty exists, re-`Locate` the pane now and `Activate` it (locate
failure is silently skipped, activate failure is fatal). At least one must act.
Order is WM first, then pane.

### Remote focus execution (`Navigator.Focus`, `navigator.go:79-135`)
Liveness in the aggregate view → not headless → WM available → registry
resolve → local route resolve → **activate pane first** → re-resolve and
require the same stable `(gui, pane)` (`RevalidateAfterActivation`,
`local.go:148-157`) → `wm.Focus` on the revalidated window. Order is pane
first, then WM, the inverse of the local path, because activating the pane can
change which window the marker names.

### Chip labels for pick (`internal/label`)
`RawName` precedence for Claude: remote `ResolvedName` → `~/.claude/sessions/<pid>.json`
`name` → normalized wezterm window title → cwd basename → `pid N`
(`label.go:56-103`). Codex never uses the terminal title. `Chip` prefixes via
`projectname`. Not navigation logic, but `pick` output is what fzf/rofi feed
back into `focus`.

## 7. Dependencies

**In (who calls this tier):** `cmd/switchboard/main.go` (discovery →
`Resolve`; WM loop; reconciler), `internal/rpc` (focus, pane-bind/state),
`internal/federation` (navigator, workspace index, routes),
`cmd/switchboard-ctl` (policy), `cmd/switchboard-waybar` / polybar (labels,
`Focused`), `internal/conformance` (contract tests).

**Out (what this tier depends on):** `internal/osproc`/`proc` (tty, pid
liveness), `internal/state` (Session, Store), `internal/history` (focus
events), `internal/projectname` + `internal/transcript` (labels),
`golang.org/x/sys/unix` (kill(0), SO_PEERCRED, tty open), `github.com/jezek/xgb`
(X11), external binaries `wezterm`, `tmux`, the Hyprland/i3 sockets, and the
WezTerm Lua integration installed by the user.

## 8. Invariants and hazards

Invariants (mined from tests and docs):
- Event addresses compare equal to `Clients()` addresses; the backend owns
  normalization (`TestHyprlandManagerConformance`, decisions #1).
- `Available()` never spawns, blocks, or panics.
- Unknown tty → `(nil, nil)`, never an error or a hang (Locator contract).
- Single and batch locate agree for the same tty
  (`TestLocatorContractShouldPassWhenBatchAndSingleAgree`, `TestReconcileFromMatchesReconcile`).
- One failing locator never blanks healthy ones (`TestChainErrorDoesNotBlankHealthy`,
  `TestChainSnapshotShouldKeepHealthyPanesWhenALocatorErrors`).
- A miss never clears an established mapping; ambiguity keeps the prior address.
- Inner locator wins a tty both own (`TestChainSnapshotShouldPreferTheInnerLocatorWhenBothOwnATTY`).
- Spinner glyphs never affect the join (`TestMatchUniqueClientNormalizesActivitySpinners`,
  `TestNormalizeIsIdempotent`).
- Enumeration happens before the store lock; `resolveTurn` is taken before `store.Apply`.
- Layout events are debounced; focus and close events never are; a pending
  layout flush lands before any focus/close event.
- Registry: candidate before first live snapshot is not navigable; a
  previously-live candidate omitted by a complete snapshot is pruned; PID reuse
  cannot reuse an old binding; multiple panes for one exact session fail closed
  (`registry_test.go` names).
- Remote focus: activation failure never raises the window; a pane missing
  after activation prunes without raising (`navigator_test.go`).
- OSC announce: open → revalidate → write once; bounded attempts; partial write
  terminated with BEL.
- `attention` never climbs more than one layer, never targets headless or
  focused rows, and a singleton urgent session toggles up and back.

Hazards:
- **`Focused` is window-level.** Two panes in one window both read focused
  (latency report D5). `is_active` is parsed and discarded.
- **Title join races the spinner.** Normalization fixed the glyph; a genuine
  title change between the two reads still costs a tick.
- **Equal titles without the marker** leave both sessions unmapped indefinitely.
- **tmux has no WM raise**; a tmux-in-wezterm session focuses the pane but not
  the OS window.
- **X11 never emits window-closed**; teardown relies on process death.
- **i3 without `$DISPLAY`/`XAUTHORITY` in the daemon environment** yields PID 0
  for every window and Observe-only silently (i3-polybar-plan F1/F2).
- **`/proc/<pid>` check in `wezterm.Muxes`** is Linux-only in an untagged file.
- **Hyprland Lua dialect** coupling: `focusBatch` breaks on a Hyprland with the
  legacy parser, and the `;` separator constraint is implicit.
- **Per-call subprocess forks**: each tick forks `wezterm cli list` once per
  mux; before batching this was 71% of daemon CPU (`main.go:806-812`).
- **Legacy `focus` RPC** still exists with the bare-number PID/index collision.
- **Snapshot shares pointer fields** (decisions #5); `cloneSessionForReconcile`
  exists to avoid mutating through them.
- **Stale enumeration ordering** is prevented only by `resolveTurn`; forgetting
  it reintroduces "older observation lands last".
- **WezTerm Lua** runs only the first `format-window-title` handler; a user
  with an existing handler must compose manually or the marker path never fires.

## 9. Essential vs accidental

Essential:
- The two seams as *traits* with `none` implementations and the tier model
  (`navigate = terminal && wm`).
- tty as the process→pane join key; `(mux pid, marker)` as the pane→window join.
- Fail-closed on ambiguity, miss-never-clears, one enumeration per tick,
  enumeration outside the state lock, ordered writes.
- Neutral three-kind event vocabulary with address normalization owned by the backend.
- Hyprland backend (the primary target), wezterm backend, and the marker
  integration. These are the only fully live-verified paths.
- Bounded attention ring and unrestricted cycle as pure functions over a snapshot.
- Exact `(host, pid, started_at)` identity for every focus request.
- The pane-bind registry semantics for remote sessions (candidate → live →
  pruned; pane identity excludes window id).

Accidental / droppable / unifiable:
- **`terminal.auto` re-detect-on-every-call** is a workaround for the boot
  race. In Rust, a `watch`-style re-probe on a timer or on a `layout` event is
  cleaner than re-probing inside every `Locate`.
- **`Resolve` vs `Reconcile` vs `ReconcileFrom` vs `findWindow`**: four entry
  points for one algorithm. Collapse to one pure `join(session, panes, clients)`
  plus one `enumerate()`; keep only the batch path and make the per-session
  path a batch of one.
- **Legacy `(pid, title)` join**: keep only as an explicit fallback flag; the
  marker should be the primary contract and documented as required for Navigate
  under wezterm.
- **`state.HyprlandInfo` / `WeztermInfo` naming** on the wire: rename to
  neutral `window` / `terminal` blocks (the v0.2 requirements already say so).
- **Legacy `focus` RPC + `pickSession`**: drop; ctl already uses `focus-session`.
- **X11 backend**: verified only for the read path against XWayland. Ship it
  behind a feature flag or defer; the i3 backend now hydrates PIDs, which
  covers the i3 case that motivated X11.
- **i3 PID hydration through a second X11 connection** is a wart of i3's IPC;
  keep, but isolate as an `i3` post-processing step so sway stays pure.
- **Hyprland no-warp batch**: keep the behavior, but the option-read-then-batch
  dance is Hyprland-version-specific; version-gate it.
- **`wezterm cli` subprocesses**: wezterm's mux socket speaks a codec that is
  not stable; keep the CLI but run one `list` per mux via a small worker with
  its own timeout rather than inline in the reconcile.
- **`label` package**: display-only; belongs to the render tier. Its
  `NameCache` is an I/O optimization that a Rust daemon can replace with an
  inotify/fs-watch on `~/.claude/sessions`.
- **Two focus orderings** (local WM-then-pane, remote pane-then-WM): unify on
  pane-then-revalidate-then-WM for both once local sessions also carry markers.

## 10. Rust mapping sketch

```rust
// crate switchboard-navigate (or modules under the daemon crate)

pub struct WindowRef(String);           // opaque, backend-owned
pub struct Window { addr: WindowRef, pid: Option<u32>, title: String,
                    workspace: String, workspace_id: i32 }
pub enum WmEvent { FocusChanged(Option<WindowRef>), WindowClosed(WindowRef), LayoutChanged }

#[async_trait]
pub trait WindowManager: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;                       // sync, env/stat only
    async fn clients(&self) -> Result<Vec<Window>, WmError>;
    async fn active_window(&self) -> Result<Option<WindowRef>, WmError>;
    async fn focus(&self, w: &WindowRef) -> Result<(), WmError>;   // WmError::Unsupported for none
    async fn subscribe(&self, cancel: CancellationToken) -> Result<mpsc::Receiver<WmEvent>, WmError>;
}

pub struct Tty(String);
pub enum PaneHandle { Wezterm { mux_pid: u32, mux_socket: PathBuf, pane_id: u32, window_id: u32, tab_id: u32 },
                      Tmux { pane: String } }
pub struct PaneRef { handle: PaneHandle, mux_pid: Option<u32>, title: String,
                     window_title: String /* normalized */, tty: Tty, cwd: Option<PathBuf> }

#[async_trait]
pub trait Terminal: Send + Sync {
    fn name(&self) -> &'static str;
    fn available(&self) -> bool;
    async fn snapshot(&self) -> Result<HashMap<Tty, PaneRef>, TermError>;   // the only enumeration
    async fn activate(&self, p: &PaneRef) -> Result<(), TermError>;
}
// `locate(tty)` becomes `snapshot().get(tty)`; Chain merges snapshots inner-first.
```

Modules:
- `navigate::wm::{hyprland, i3, x11, none}` — one file per backend, plus
  `wire::i3` (framing) and `wire::hyprland` (socket paths, instance discovery,
  event line parser).
- `navigate::terminal::{wezterm, tmux, chain, none}` and `terminal::detect`.
- `navigate::join` — pure: `fn join(pane: &PaneRef, clients: &[Window]) ->
  Option<Window>` with the marker/legacy rule and `title::normalize`.
- `navigate::panebind::{payload, osc, registry, local_route}` — pure types +
  one async route resolver.
- `navigate::focus` — the two execution orders and the attention/cycle policy as
  pure functions `fn next_attention(&[SessionView]) -> Option<ExactKey>`.
- `navigate::detect` — precedence table, returns `Box<dyn WindowManager>` and
  `Box<dyn Terminal>`, republishes capabilities.

Crates:
- **Hyprland: hand-roll.** The wire is two Unix sockets with a text protocol;
  `hyprland-rs` pulls in a large surface, is version-chasing, and the Lua
  dispatch dialect plus lock-file instance discovery are not in it. ~300 lines
  with `tokio::net::UnixStream` + `tokio::io::BufReader::lines`.
- **sway/i3: `swayipc-async`** is solid and small; it covers framing,
  GET_TREE, SUBSCRIBE. Use it. Keep the i3 `_NET_WM_PID` hydration on
  `x11rb` (async, pure Rust) — the same crate serves the X11 backend.
- **X11: `x11rb`** (with `allow-unsafe-code` off). EWMH atoms are a handful of
  `intern_atom` + `get_property` calls.
- **wezterm / tmux: `tokio::process::Command`** with `tokio::time::timeout`;
  `serde_json` for the wezterm rows. Peer-cred check via `nix::sys::socket::getsockopt(PeerCredentials)`.
- **panebind OSC**: `nix::fcntl::open` with `O_NONBLOCK|O_NOCTTY|O_NOFOLLOW|O_CLOEXEC`
  and `nix::unistd::write` in a bounded loop; `base64`.
- **Registry**: `parking_lot::RwLock<…>` around two `HashMap`s; keep the
  exact same operations and tests.

Sync vs async:
- WM subscriptions and subprocess forks are naturally async under tokio; keep
  `available()` sync because detection must be a bare stat/env read.
- The join, title normalization, attention ring, cycle, selector parsing, and
  registry are pure and synchronous; put them in modules with no tokio import
  so the Go table tests port 1:1.
- Preserve the "enumerate outside the lock, then apply under a short lock"
  shape: `async fn enumerate() -> Snapshots` then a sync `apply(&mut Store, &Snapshots)`.
  Replace `resolveTurn` with a single `tokio::sync::Mutex<()>` around
  enumerate+apply, or better, a single "resolver" task that owns the ticker and
  a `Notify` for layout bursts, so ordering is structural rather than locked.
- Debounce: a single task with `tokio::select!` over the event receiver and a
  `tokio::time::Sleep` armed on the first layout event, flushed before any
  focus/close event, exactly as `drainWMEvents`.
- Carry `is_active` from wezterm onto `PaneRef` and compute a pane-level
  `focused` bit from day one; that removes the "skip every focused row"
  workaround in the attention ring.
