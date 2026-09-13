/* Source-backed architecture facts. This file is data, not a Rust emulator. */
window.SB_SYSTEM = {
  revision: '17ff0c9',
  nodes: [
    {
      id: 'hook', title: 'Hook forwarder', subtitle: 'one short-lived process per hook', layer: 'source', status: 'implemented',
      summary: 'The agent hands JSON to switchboard-hook on stdin. The forwarder hashes tool input, removes seven known content fields, and sends a JSON line.',
      details: ['Claude hook and statusline modes exist. This binary does not yet select a provider.', 'The forwarded pid is the parent of the forwarder, so RPC must resolve its ancestry.', 'Read/write socket timeouts are 500 ms each. Connect and stdin reading are not bounded by that timeout; this is a known gap.', 'The hook path removes named fields; it is not a complete allowlist of every possible future payload field.'],
      code: 'let pid = std::os::unix::process::parent_id();\nlet frame = scrub(payload, event, pid);\nclient::send(&socket_path(), &frame, TIMEOUT)',
      source: {path: 'src/bin/switchboard-hook.rs', line: 24}, types: ['HookFrame', 'InputHash']
    },
    {
      id: 'socket', title: 'RPC listener', subtitle: 'Unix socket · JSON lines', layer: 'runtime', status: 'implemented',
      summary: 'Decode the request, resolve the tracked ancestor, call the Claude adapter, and offer its inputs to the inbox.',
      details: ['One Tokio task per accepted connection; the socket is mode 0600.', 'Commands are hook, statusline, list and subscribe. HookFrame has no provider field.', 'Hook and statusline delivery use try_send. The reply is still ok if the inbox dropped an input.', 'The current connection has a 1 MiB total read cap, but no read deadline. Accept errors retry immediately.'],
      code: 'let process = resolve(&proc_root, frame.pid, &view).await;\nlet ctx = Ctx { now: now(), process };\noffer(&inbox, claude::hook(&frame, &ctx));\nok_reply()',
      source: {path: 'src/daemon/rpc.rs', line: 74}, types: ['Request', 'HookFrame', 'StatuslineFrame', 'Ctx']
    },
    {
      id: 'scanner', title: 'Process scanner', subtitle: 'every 1 second', layer: 'source', status: 'implemented',
      summary: 'Read Linux process facts and Claude session files, then send a process snapshot followed by Died events for missing keys.',
      details: ['Blocking filesystem work runs through spawn_blocking.', 'ProcKey is (pid, start). Reusing a pid produces a different identity.', 'The scanner sends with send().await, so a full inbox slows scanning rather than dropping its results.', 'The current classifier recognizes interactive Claude. Codex and portable process backends remain unwired.', 'A failed sweep currently looks like an empty successful sweep; the resulting false deaths are an audited bug.'],
      code: 'let (inputs, seen) = sweep_inputs(&known, rows, at);\nfor input in inputs {\n    inbox.send(input).await?;\n}\nknown = seen; // shortened error handling',
      source: {path: 'src/daemon/scanner.rs', line: 26}, types: ['ProcKey', 'ProcessRow', 'Snapshot', 'EventKind']
    },
    {
      id: 'watcher', title: 'Transcript watcher', subtitle: 'one per process · 200 ms poll', layer: 'source', status: 'implemented',
      summary: 'Own a file and Tail cursor, decode complete JSON lines, and run the Claude transcript adapter.',
      details: ['First opening replays at most the last 128 KiB, starting on a whole-line boundary.', 'The first successful poll uses Inferred confidence; subsequent polls use Confirmed.', 'The cursor and file move into spawn_blocking and back. No shared lock is needed.', 'Inputs use send().await. A busy inbox pauses delivery while retaining the already-read batch.', 'A session rotation replaces this task; process death aborts it. Replay currently travels as ordinary inputs and can write old transitions into history.'],
      code: 'let ctx = LineCtx { process, session, account, now, confidence };\nlet inputs = polled.lines.iter()\n    .filter_map(|text| serde_json::from_str::<Value>(text).ok())\n    .flat_map(|v| line(&v, &ctx));',
      source: {path: 'src/daemon/watcher.rs', line: 70}, types: ['WatchSpec', 'Tail', 'Polled', 'LineCtx', 'Confidence']
    },
    {
      id: 'adapters', title: 'Pure adapters', subtitle: 'source facts → shared vocabulary', layer: 'adapter', status: 'implemented',
      summary: 'Interpret raw fields as events and snapshots. Adapters say what happened and who caused it; they never choose a chip color.',
      details: ['hook, transcript and statusline adapters are present under ingest/claude.', 'Each returns Vec<Input>: one Stop can produce both TurnEnded and a Waits snapshot.', 'Ctx supplies receipt time and process identity; LineCtx also carries the known session, account and replay confidence.', 'Unsupported hook names yield no inputs. Tolerance is intentional; unknown enum handling is not yet uniform.', 'Codex hooks/app-server and surface/remote adapters belong in the extension plan. They are not active paths in this golden.'],
      code: 'pub enum Input {\n    Event(Event),\n    Snapshot(Snapshot),\n}\n// Example entry point:\npub fn hook(frame: &HookFrame, ctx: &Ctx) -> Vec<Input>',
      source: {path: 'src/ingest/mod.rs', line: 23}, types: ['Input', 'Event', 'Snapshot', 'Cause', 'Confidence']
    },
    {
      id: 'inbox', title: 'Input inbox', subtitle: 'mpsc<Input> · capacity 4096', layer: 'runtime', status: 'implemented',
      summary: 'Many producers transfer owned inputs to one consumer. This is the point where concurrent observations become a single sequence of reducer calls.',
      details: ['RPC offers with try_send: full means drop and log.', 'Scanner and transcript watchers use send().await: full means backpressure.', 'Channel order is arrival order, not universal event-time order. Sources can race; identities and scoped freshness checks still matter.', 'PermissionRequest is not recoverable from a transcript, so shedding every hook indiscriminately is not a safe general contract.'],
      code: 'let (inbox_tx, inbox_rx) =\n    mpsc::channel(settings.inbox_capacity);\n// default: 4096 Input values',
      source: {path: 'src/daemon/mod.rs', line: 75}, types: ['Input']
    },
    {
      id: 'core', title: 'Core task', subtitle: 'the only owner of Board', layer: 'runtime', status: 'implemented',
      summary: 'Wait for the next input or the earliest board deadline. Apply the reducer synchronously, then dispatch its effects.',
      details: ['Board is moved into this task. No other task gets a mutable reference to it.', 'No periodic core tick: next_deadline is recomputed on each loop. With no deadline, only an input wakes it.', 'apply and tick perform no filesystem, socket or clock reads. The runtime supplies time.', 'PublishState stays in the core; all other effects enter the unbounded effect channel.', 'The core does not await publishers, disk writes, or an effect runner.'],
      code: 'let produced = tokio::select! {\n    input = inbox.recv() => reduce::apply(&mut board, input),\n    () = wake => reduce::tick(&mut board, now()),\n};\ndispatch(&board, produced, &view, &effects);\n// shortened Option/shutdown handling',
      source: {path: 'src/daemon/core.rs', line: 22}, types: ['Board', 'Input', 'Effect']
    },
    {
      id: 'board', title: 'Board + Agent', subtitle: 'independent facts, one local host', layer: 'model', status: 'implemented',
      summary: 'The board stores agents by process identity and shared limits by account. Each agent holds several independently updateable pieces of state.',
      details: ['Activity contains Unobserved, Idle, or Turn; Turn owns a BTreeMap<ToolUseId, Tool>.', 'Prompts, subagents, waits and compaction are separate axes. A tool gate belongs to its call.', 'Account limits are looked up through Agent.account, so one limit can affect several agents.', 'A session can rotate while ProcKey remains stable. A hook naming an unknown agent does not create one.', 'The separate Fleet type is only a model sketch: remote rows must not enter local discovery or liveness.'],
      code: 'pub struct Board {\n    pub agents: BTreeMap<ProcKey, Agent>,\n    pub accounts: BTreeMap<AccountId, Account>,\n}\n// Agent owns activity, prompts, subagents, waits,\n// compaction, placement, beat and Shown.',
      source: {path: 'src/model.rs', line: 528}, types: ['Board', 'Agent', 'Activity', 'Tool', 'Gate', 'Prompts', 'Subagents', 'Account']
    },
    {
      id: 'fold', title: 'Phase → Shown', subtitle: 'derive truth, then settle display', layer: 'model', status: 'implemented',
      summary: 'Agent::phase combines the facts by precedence. Shown::settle is the single rule for changing the displayed phase.',
      details: ['Precedence: human gate → rejected limit or failure → interrupted turn → unowned gate → active turn → unobserved → active child → wait → idle.', 'Explorer palette: Attention is red; Idle, Blocked and Stopped are amber; Unknown is grey; Working, Delegating and Waiting are green. The current golden still maps Idle to Lamp::Green; that mapping needs the same correction.', 'De-escalations, same-severity moves, user-caused changes and Confirmed evidence are immediate.', 'Only a louder agent-caused change waits: 10 seconds for Inferred, 6 seconds for Snapshot.', 'Current cause/confidence come from the triggering input. An unrelated confirmed event can incorrectly skip a pending dwell; the audit calls for evidence tied to the derived result.'],
      code: 'let truth = agent.phase(limits);\nagent.shown.settle(truth, cause, confidence, now);\n\n// Shown stores the display state, not a second truth.\npub struct Shown {\n    pub phase: Phase, pub since: Instant,\n    pub pending: Option<Escalation>,\n}',
      source: {path: 'src/model.rs', line: 1181}, types: ['Phase', 'Severity', 'Lamp', 'Shown', 'Escalation', 'Confidence']
    },
    {
      id: 'view', title: 'Published View', subtitle: 'watch<Arc<View>> · latest value', layer: 'output', status: 'implemented',
      summary: 'Render a small read-only view and replace the watch value only when it differs. Consumers never borrow Board.',
      details: ['View contains Vec<AgentView>, including shown, truth, reason and since.', 'since stays an Instant internally. Serialization computes epoch time later, so wall-clock timestamps do not force needless watch updates.', 'Reasons are built eagerly for every agent in View::render; no separate tooltip/detail fetch exists.', 'watch retains the latest value, not a queue of every transition. A slow subscriber can miss intermediate views.', 'Truth and reason changes can publish even while shown remains unchanged. Reasons currently explain truth and may differ from the displayed lamp during a dwell.'],
      code: 'let next = View::render(board);\nview.send_if_modified(|current| {\n    if **current == next { return false; }\n    *current = Arc::new(next);\n    true\n});',
      source: {path: 'src/daemon/core.rs', line: 80}, types: ['View', 'AgentView', 'Phase', 'Reason']
    },
    {
      id: 'effects', title: 'Effect runner', subtitle: 'unbounded mpsc<Effect>', layer: 'runtime', status: 'implemented',
      summary: 'Turn requests for work into watcher ownership changes and best-effort history delivery. It never changes Board.',
      details: ['WatchTranscript replaces the task stored under ProcKey; Forget aborts it.', 'A Watchers map owns JoinHandle values and aborts all remaining tasks on drop.', 'The runner currently constructs a Claude transcript path from cwd and session, and assigns the default account.', 'AppendHistory is offered to a separate bounded channel of 512 records; full means drop and log.', 'The incoming effect lane is unbounded. It protects the core from waiting, but does not guarantee bounded memory under sustained overload.'],
      code: 'match effect {\n    WatchTranscript { process, session, cwd } => { /* start/replace */ }\n    Forget { process } => watchers.stop(&process),\n    AppendHistory(entry) => { history.try_send(entry); }\n    PublishState => {} // handled in core\n}',
      source: {path: 'src/daemon/effects.rs', line: 19}, types: ['Effect', 'WatchSpec', 'Watchers', 'HistoryEntry']
    },
    {
      id: 'publisher', title: 'state.json', subtitle: '250 ms coalesce · atomic rename', layer: 'output', status: 'implemented',
      summary: 'After a view change, wait 250 ms, serialize the newest view, and replace the state file atomically.',
      details: ['This is a fixed coalescing window from the first change, not a debounce extended by every later change.', 'Writes are sequential. Readers see either the old file or the complete new file.', 'The schema is switchboard-rust/1 with updated_ms and agents. Each agent has 11 serialized fields.', 'No periodic file write is needed solely to advance a displayed age; clients can compute age from since_ms.', 'No Rust bar, renderer, navigation endpoint or shared-memory frame is wired here.'],
      code: 'view.changed().await?;\nsleep(coalesce).await;\nlet latest = Arc::clone(&view.borrow_and_update());\nlet body = latest.to_json(now(), SystemTime::now());\nwrite_atomic(&path, body.to_string().as_bytes()).await?;\n// shortened error handling',
      source: {path: 'src/daemon/publish.rs', line: 17}, types: ['View', 'AgentView']
    },
    {
      id: 'subscriber', title: 'RPC clients', subtitle: 'list once · subscribe on change', layer: 'output', status: 'implemented',
      summary: 'A list request returns the full current snapshot. Subscribe sends it immediately, then sends the newest full snapshot after each observed change.',
      details: ['RPC frames wrap the same document schema as state.json in {"snapshot": …}.', 'There is no delta protocol and no lazy detail endpoint. reason and the other agent fields travel in each frame.', 'Subscribers bypass the disk publisher’s 250 ms window.', 'A slow socket write stalls only that connection; watch coalesces intervening views.', 'This channel carries rendered state, not raw inputs or history records.'],
      code: 'loop {\n    let line = snapshot_line(&view.borrow_and_update());\n    write_line(&mut write, &line).await?;\n    view.changed().await?;\n}\n// shortened error handling',
      source: {path: 'src/daemon/rpc.rs', line: 96}, types: ['Request', 'View', 'AgentView']
    },
    {
      id: 'history', title: 'History appender', subtitle: 'bounded 512 · JSONL by UTC day', layer: 'output', status: 'implemented',
      summary: 'Append agent-found, agent-gone and displayed-phase changes. History is downstream evidence, not the authoritative live board.',
      details: ['HistoryEntry::Shown stores from, to and held: the duration the previous phase was displayed.', 'The appender adds SystemTime when consuming the record, then performs its append through spawn_blocking.', 'The queue is deliberately best effort: full means a lost history record, not a stalled core.', 'Golden history does not contain every input, tool call, usage sample or UI detail.', 'The guide’s annealing, normalized geometry and dashboard delta stream are future work.'],
      code: 'pub enum HistoryEntry {\n    AgentFound { process: ProcKey, cwd: PathBuf },\n    AgentGone { process: ProcKey, session: Option<SessionId> },\n    Shown { process: ProcKey, session: Option<SessionId>,\n            from: Phase, to: Phase, held: Duration },\n}',
      source: {path: 'src/model.rs', line: 1335}, types: ['HistoryEntry', 'Record', 'HistoryWriter']
    }
  ],
  edges: [
    {from: 'hook', to: 'socket', label: 'one JSON line', kind: 'event'},
    {from: 'socket', to: 'adapters', label: 'HookFrame / StatuslineFrame + Ctx', kind: 'event'},
    {from: 'scanner', to: 'inbox', label: 'Processes + Died · send().await', kind: 'snapshot'},
    {from: 'watcher', to: 'adapters', label: 'JSON line + LineCtx', kind: 'event'},
    {from: 'adapters', to: 'inbox', label: 'Vec<Input>', kind: 'event'},
    {from: 'inbox', to: 'core', label: 'one Input', kind: 'event'},
    {from: 'core', to: 'board', label: 'apply / tick mutate facts', kind: 'event'},
    {from: 'board', to: 'fold', label: 'Agent + account Limits', kind: 'view'},
    {from: 'fold', to: 'view', label: 'facts + Shown → View::render', kind: 'view'},
    {from: 'core', to: 'effects', label: 'Effect · unbounded', kind: 'effect'},
    {from: 'effects', to: 'watcher', label: 'WatchTranscript / Forget', kind: 'effect'},
    {from: 'effects', to: 'history', label: 'AppendHistory · try_send', kind: 'effect'},
    {from: 'view', to: 'publisher', label: 'latest view · 250 ms window', kind: 'view'},
    {from: 'view', to: 'subscriber', label: 'latest view · on change', kind: 'view'}
  ],
  boundaries: [
    {title: 'Raw bytes stop at adapters', summary: 'The forwarder strips known content fields; pure source adapters translate facts into shared Input values. Provider vocabulary belongs at this edge, not in the phase fold.', code: 'fn hook(frame: &HookFrame, ctx: &Ctx) -> Vec<Input>', source: {path: 'src/ingest/claude/hook.rs', line: 47}},
    {title: 'Events and snapshots merge differently', summary: 'An Event describes an edge with actor, cause and confidence. A Snapshot describes a scope at taken. Scoped freshness protects newer event facts; this is implemented for subagent snapshots, not universally enforced for process discovery.', code: 'AsOf { value, as_of }.supersedes(last_event)\n// true only if as_of > last_event', source: {path: 'src/model.rs', line: 486}},
    {title: 'One task owns mutable live state', summary: 'Only core owns Board. Every input moves through mpsc; all readers get Arc<View>. This removes the need for a lock around the board without implying that channels use no synchronization internally.', source: {path: 'src/daemon/core.rs', line: 22}},
    {title: 'Facts, truth and display are distinct', summary: 'Activity/gates/waits store facts. Agent::phase derives truth. Shown stores the display and a pending escalation. A display change does not rewrite the facts that caused it.', code: 'facts → phase(limits) → shown.settle(…) → View', source: {path: 'src/model.rs', line: 1004}},
    {title: 'Effects ask; runtime performs', summary: 'The reducer returns data describing I/O work. Starting a watcher or appending history happens outside it. Anything learned comes back as a fresh Input.', code: 'fn apply(board: &mut Board, input: Input) -> Vec<Effect>', source: {path: 'src/reduce/mod.rs', line: 53}},
    {title: 'Backpressure is a per-channel policy', summary: 'Inbox: 4096, mixed wait/drop producers. Effects: unbounded. History: 512, drop on full. View: one latest value. These are four different tradeoffs, not one global queue rule.', source: {path: 'src/daemon/mod.rs', line: 75}},
    {title: 'Data rate is separate from animation rate', summary: 'The current daemon publishes when View changes, not at 60 Hz. Consumers can animate locally from phase and since_ms. A shared-memory bar frame is a guide proposal; golden currently serves JSON only.', source: {path: 'src/view.rs', line: 65}},
    {title: 'Local Board, remote Fleet, history', summary: 'The guide separates actionable local agents, read-only remote snapshots, and retrospective history. Fleet types exist but have no runtime producer. History append exists; annealing and dashboard geometry do not.', code: 'pub struct Fleet {\n    pub local: Board,\n    pub remote: BTreeMap<Host, AsOf<Vec<Row>>>,\n}', source: {path: 'src/model.rs', line: 1803}},
    {title: 'The core vocabulary is the extension seam', summary: 'The intended architecture is provider agnostic and cross-platform. Source selection, runtime capabilities and dispatch still need design work. A static provider table, traits and generics are options discussed, not a settled implementation choice.', source: {path: 'src/ingest/mod.rs', line: 1}}
  ],
  gaps: [
    {title: 'Provider and platform extensibility', status: 'decision', current: 'Provider has Claude/Codex variants, but discovery, hooks, transcript following and Unix transport are hardwired to Claude/Linux paths.', proposal: 'Support Claude and Codex in an open source architecture; choose provider dispatch and platform boundaries from maintenance needs. The user explicitly rejected a closed provider scope.', source: {path: 'src/proc.rs', line: 106}},
    {title: 'Rolling source capabilities', status: 'decision', current: 'Adapters tolerate some unknown fields, but unknown enum defaults can imply a false known meaning. There is no capability discovery or doctor report.', proposal: 'Detect emitted and subscribed signals, retain weaker fallbacks, and test captured shapes. Whether capabilities live in model data or only source wiring remains a decision.', source: {path: 'src/ingest/claude/hook.rs', line: 47}},
    {title: 'Clock, limits and current evidence', status: 'known-bug', current: 'Rejected limits do not expire at resets_at. tick receives only Instant. The statusline adapter expects an older limits shape, and Beat is stored without correcting activity.', proposal: 'Handle foreign wall-clock deadlines and sleep/restart reconciliation. Validate newer Claude live-status and limits payloads with the capture harness before giving them authority.', source: {path: 'src/reduce/tick.rs', line: 13}},
    {title: 'Identity and death', status: 'known-bug', current: 'An unreadable sweep can kill all agents; an old process snapshot can resurrect one. Nested headless Claude hooks can resolve to the wrong tracked ancestor.', proposal: 'Distinguish observation failure from confirmed absence, stop ancestry at a recognized agent boundary, and enforce process/session freshness. Tombstones are one candidate, not an accepted design.', source: {path: 'src/daemon/rpc.rs', line: 143}},
    {title: 'Delivery under load', status: 'known-bug', current: 'RPC drops every kind of input when the inbox fills. A permission request has no transcript backstop. Connect has no deadline and accept errors can spin.', proposal: 'Choose a delivery policy for irreplaceable edges, bound client operations, and back off on accept errors while preserving the agent’s responsiveness.', source: {path: 'src/daemon/rpc.rs', line: 130}},
    {title: 'Evidence and explanation disagree', status: 'known-bug', current: 'phase(), settle() and view::reason derive related answers separately. A confirmed unrelated event can bypass a dwell; some tooltips do not match their truth phase.', proposal: 'Explore a single derived verdict carrying phase, reason and supporting evidence. This is a redesign proposal, not an existing Rust type.', source: {path: 'src/view.rs', line: 108}},
    {title: 'Restart reconstruction and unowned gates', status: 'decision', current: 'Discovery begins Unknown, then replays transcript lines as ordinary inferred inputs. Replay can flood history and cannot establish that an old prompt is still open. Unknown-owned gates currently yield grey immediately.', proposal: 'Decide how to request fresh evidence after restart and handle approvals whose human/machine owner is unknown. Batch replay and live-status reconciliation are candidates; no red-after-grace policy has been accepted.', source: {path: 'src/daemon/watcher.rs', line: 45}},
    {title: 'Subagent lifecycle and per-call gates', status: 'known-bug', current: 'Each Subagent has one optional gate. Settling any call can clear it. Panel replacement logic exists, but no panel producer is wired.', proposal: 'Use captures to establish authoritative child snapshots and expiry rules; fix call-level gate ownership without assuming all providers expose identical child signals.', source: {path: 'src/model.rs', line: 878}},
    {title: 'Surfaces, navigation and federation', status: 'planned', current: 'Placement, Row, Reach and Fleet types exist. No WM locator, navigation RPC or remote task feeds them. focused normally remains false.', proposal: 'Keep desktop adapters and remote observations independent of provider state machines. Remote snapshots belong to Fleet and never become locally owned processes.', source: {path: 'src/model.rs', line: 1742}},
    {title: 'Output and retrospective models', status: 'decision', current: 'Golden exposes switchboard-rust/1 JSON and best-effort history. It has no renderer, Go-v3 shim, shared-memory bar frame or dashboard materializer.', proposal: 'Choose output fields for actual consumers. The guide’s downstream plan separates live chips, local animation, and annealed history/geometry; compatibility and detail-fetch design remain open.', source: {path: 'src/view.rs', line: 65}}
  ]
};
