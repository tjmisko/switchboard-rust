/* Curated traces of the checked-in golden implementation, not a Rust emulator.
 * Identifiers, timestamps, snippets and JSON values are illustrative. Rust snippets
 * show real type/variant names; `..` means omitted fields, not executable code.
 * Source locations refer to the golden worktree. No live hooks or daemon are run.
 */
(() => {
  const state = (phase, rest = {}) => ({
    phase, shown: phase, turn: "Idle", tools: "0", gates: "0", children: "0", ...rest
  });
  const step = (title, node, route, explanation, label, code, current, effects = [], note) => ({
    title, node, route, explanation, wire: { label, code }, state: current, effects,
    ...(note ? { note } : {})
  });
  const refs = (...pairs) => pairs.map(([path, line]) => ({ path, line }));

  window.SB_SCENARIOS = [
    {
      id: "permission", title: "A tool needs your approval", subtitle: "Hook → typed event → joined gate → red → green",
      status: "implemented",
      summary: "Follow one Claude tool call through the real gate model. The example begins with an already discovered agent in session s1. Confirmed permission evidence goes red immediately; this case has no dwell.",
      source: refs(["src/ingest/claude/hook.rs", 47], ["src/reduce/event.rs", 315], ["src/reduce/event.rs", 368], ["src/model.rs", 1054], ["src/view.rs", 41]),
      steps: [
        step("Claude dispatches a tool", "hook", ["hook", "socket"],
          "The forwarder sends one scrubbed JSON object plus a newline. The process and session identify the agent; the tool call ID identifies this call. Values here are illustrative.",
          "Illustrative wire frame", `{"cmd":"hook","event":"PreToolUse","pid":4036,
 "session_id":"s1","tool_use_id":"call-7","tool_name":"Bash",
 "tool_input_hash":"0123456789abcdef"}`, state("Idle")),
        step("The edge produces a typed event", "adapters", ["socket", "adapters", "inbox"],
          "decode_request builds a HookFrame. resolve looks up the tracked ancestor. claude::hook translates the frame into the shared vocabulary and assigns cause and confidence.",
          "Rust · abbreviated Event", `Input::Event(Event {
    process: Some(ProcKey { pid: 4036, start: 91000 }),
    session: "s1".into(), actor: Actor::Agent,
    cause: Cause::Agent, confidence: Confidence::Confirmed,
    kind: EventKind::ToolDispatched {
        call: "call-7".into(), tool: "Bash".into(), hash,
    }, ..
})`, state("Idle"), [],
          "RPC uses try_send: an accepted socket frame is not a guarantee that a full inbox retained the input."),
        step("The core opens the call", "board", ["inbox", "core", "board", "fold"],
          "Only the core mutates Board. dispatch creates Activity::Turn if needed, then inserts a Tool keyed by ToolUseId. A running call makes the derived phase Working.",
          "Rust · model fragment", `Activity::Turn {
    tools: { "call-7" => Tool {
        name: "Bash".into(), hash, since: now,
        gate: Gate::Running,
    } }, ..
}`, state("Working", { turn: "Turn · key None", tools: "1 · call-7" }),
          ["AppendHistory(Shown { Idle → Working })", "PublishState"]),
        step("A permission prompt arrives", "socket", ["hook", "socket", "adapters", "inbox"],
          "This example uses a prompt payload without tool_use_id. The optional hash lets the reducer join the prompt to the matching tool. Current code also accepts a call ID when supplied.",
          "Illustrative wire frame", `{"cmd":"hook","event":"PermissionRequest","pid":4036,
 "session_id":"s1","tool_name":"Bash",
 "tool_input_hash":"0123456789abcdef"}`, state("Working", { turn: "Turn", tools: "1 · call-7" })),
        step("Join evidence onto the tool", "board", ["inbox", "core", "board"],
          "ask first uses an exact call ID when present. Without one it looks for a running call with the same tool and compatible hash. A prompt with no matching call is kept in Prompts::unjoined instead.",
          "Rust · gate now stored on Tool", `Gate::Blocked {
    kind: GateKind::Approval,
    since: now,
    owner: Ownership::Human,
    req: None,
}`, state("Attention", { shown: "Working", turn: "Turn", tools: "1 · call-7", gates: "1 · Human / Approval" }), [],
          "This is an explanatory pause inside one synchronous reducer call. The core cannot accept another input between mutation and settle."),
        step("Truth and shown turn red together", "fold", ["board", "fold", "view"],
          "Agent::phase checks human gates first. Shown::settle receives Confidence::Confirmed, so an escalation to Attention is immediate. The 10-second inferred dwell does not apply to this hook.",
          "Rust · the display decision", `let truth = agent.phase(limits); // Attention
agent.shown.settle(
    truth, Cause::Agent, Confidence::Confirmed, now,
); // shown = Attention, pending = None`,
          state("Attention", { turn: "Turn", tools: "1 · call-7", gates: "1 · Human / Approval" }),
          ["AppendHistory(Shown { Working → Attention })", "PublishState"]),
        step("Readers see a compact projection", "subscriber", ["view", "subscriber"],
          "View is a projection, not the whole Board. The subscriber receives a full snapshot on each observed change. The reason text is currently derived separately from Agent::phase.",
          "JSON · selected actual output fields", `{"phase":"attention","lamp":"red","truth":"attention",
 "reason":"wants approval to run Bash","focused":false}`, state("Attention", { turn: "Turn", tools: "1 · call-7", gates: "1 · Human / Approval" }), [],
          "The snippet selects fields from an agent object; the full document also carries identity, times, schema and the agents array."),
        step("A settled call removes its gate", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold", "view"],
          "After the tool completes, PostToolUse becomes ToolSettled. settle_call removes call-7 from the map, including its gate. The turn is still open, so the phase returns to Working immediately.",
          "Rust · normalized event kind", `EventKind::ToolSettled {
    call: "call-7".into(),
    outcome: ToolOutcome::Ok { ms: None },
}`, state("Working", { turn: "Turn", tools: "0 · thinking" }),
          ["AppendHistory(Shown { Attention → Working })", "PublishState"]),
        step("Stop closes the turn", "fold", ["hook", "socket", "adapters", "inbox", "core", "board", "fold", "view"],
          "A Stop hook becomes TurnEnded with TurnOutcome::Completed. With no children, waits or account limit, the agent becomes Idle. The tools map belongs to the turn and goes away with it.",
          "Rust · stored activity", `Activity::Idle {
    since: now,
    after: Some(TurnOutcome::Completed),
}`, state("Idle"), ["AppendHistory(Shown { Working → Idle })", "PublishState"])
      ]
    },
    {
      id: "dwell", title: "Truth can lead the display", subtitle: "Inferred failure → pending escalation → timer promotion",
      status: "implemented",
      summary: "This trace isolates the current Shown hysteresis rule: an inferred, agent-caused failure waits ten seconds before amber. Assume no other event arrives during the dwell. Live transcript polling uses Confirmed after its initial read.",
      source: refs(["src/model.rs", 1004], ["src/model.rs", 1054], ["src/reduce/mod.rs", 93], ["src/reduce/tick.rs", 83], ["src/daemon/core.rs", 22], ["docs/audit/repro/adv_bugs.rs", 411]),
      steps: [
        step("Read failure evidence during replay", "watcher", ["watcher", "adapters"],
          "The initial transcript read uses Confidence::Inferred. An API error line becomes a failed turn. This example has no quotaLimits field, keeping account limits out of the trace.",
          "Illustrative transcript line", `{"type":"assistant","isApiErrorMessage":true,
 "error":"rate_limit","message":{"content":[]}}`, state("Working", { turn: "Turn" })),
        step("Deliver cause and confidence as data", "inbox", ["adapters", "inbox", "core"],
          "The adapter does not decide the chip colour. It reports what happened, who caused it, and how this observation should be trusted.",
          "Rust · abbreviated Event", `Event {
    cause: Cause::Agent,
    confidence: Confidence::Inferred,
    kind: EventKind::TurnEnded {
        turn: None,
        outcome: TurnOutcome::Failed { class: ErrorClass::RateLimit },
    }, ..
}`, state("Working", { turn: "Turn" })),
        step("Store failure; arm the dwell", "fold", ["core", "board", "fold", "view"],
          "The model is immediately Idle after Failed; its derived truth is Blocked. Shown remains Working and remembers a pending escalation. View includes both shown and truth, so this can still publish a changed view before the lamp moves.",
          "Rust · displayed state", `Shown {
    phase: Phase::Working, since: previous_change,
    pending: Some(Escalation {
        to: Phase::Blocked,
        first_seen: t0,
        promote_at: t0 + Duration::from_secs(10),
    }),
}`, state("Blocked", { shown: "Working", turn: "Idle · after Failed" }), ["PublishState"]),
        step("Sleep until the earliest deadline", "core", ["fold", "core"],
          "next_deadline includes Shown::pending.promote_at. The core selects between a new input and sleep_until; it does not poll the whole Board on a fixed tick.",
          "Rust · core scheduling fragment", `tokio::select! {
    input = inbox.recv() => reduce::apply(&mut board, input),
    () = wake => reduce::tick(&mut board, now()),
}`, state("Blocked", { shown: "Working", turn: "Idle · after Failed" }), [],
          "The snippet abbreviates Option handling. Any arriving input causes the next deadline to be recomputed."),
        step("Ten seconds later, promote", "fold", ["core", "board", "fold", "view"],
          "tick re-evaluates the same Blocked truth. The existing pending deadline has elapsed, so Shown moves to Blocked and clears pending. No file or socket operation occurs inside this decision.",
          "Rust · deadline reached", `reduce::tick(&mut board, t0 + Duration::from_secs(10));
// shown.phase = Phase::Blocked
// shown.pending = None`, state("Blocked", { turn: "Idle · after Failed" }),
          ["AppendHistory(Shown { Working → Blocked })", "PublishState"]),
        step("A new user prompt starts work", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold", "view"],
          "TurnStarted replaces Idle::after(Failed) with a new turn. With no account limit, truth is Working. De-escalation is immediate regardless of confidence.",
          "Rust · normalized event kind", `EventKind::TurnStarted {
    turn: Some(TurnKey::Prompt("p2".into())),
    origin: Origin::Human,
}`, state("Working", { turn: "Turn · p2" }), ["AppendHistory(Shown { Blocked → Working })", "PublishState"],
          "Known adjacent bug: an unrelated Confirmed hook during the dwell can promote the old inferred failure early, because settle receives the latest event's confidence. Audit repro: adv_bugs.rs:418.")
      ]
    },
    {
      id: "discovery", title: "Find an agent, then follow it", subtitle: "Process identity → session identity → owned watcher",
      status: "implemented",
      summary: "Processes establish existence; hooks and transcripts establish activity. This trace also shows /clear replacing the watcher and rejecting late events from the old session. Today these I/O paths are wired to Linux and Claude.",
      source: refs(["src/proc.rs", 144], ["src/daemon/scanner.rs", 54], ["src/reduce/snapshot.rs", 141], ["src/reduce/event.rs", 45], ["src/daemon/effects.rs", 19], ["src/daemon/watcher.rs", 39]),
      steps: [
        step("A sweep finds a process", "scanner", ["scanner", "inbox"],
          "Every second the scanner reads process facts on the blocking pool. ProcKey combines PID with process start time; a recycled PID is a different identity. Here the session file is not available yet.",
          "Rust · illustrative ProcessRow", `ProcessRow {
    key: ProcKey { pid: 4036, start: 91000 },
    provider: Provider::Claude,
    cwd: "/work/demo".into(), session: None,
    tty: None, cpu_ticks: 120,
}`, state("Unknown", { turn: "Not discovered" })),
        step("Discovery creates an unobserved agent", "board", ["inbox", "core", "board", "fold", "view"],
          "SnapshotKind::Processes adds the row to Board.agents. It does not infer work from CPU or process existence. The displayed chip starts Unknown, and no watcher is started without a session.",
          "Rust · initialization", `Agent {
    activity: Activity::Unobserved { since: now },
    shown: Shown::new(Phase::Unknown, now),
    session: None, account: default_account(), ..
}`, state("Unknown", { turn: "Unobserved" }), ["AppendHistory(AgentFound)", "PublishState"]),
        step("SessionStart supplies the conversation", "hook", ["hook", "socket", "adapters", "inbox", "core", "board"],
          "A SessionStart hook identifies s1. Routing resolves the process first and falls back to a tracked session. The agent stores the session and resets conversation-local activity to Idle.",
          "Illustrative wire frame", `{"cmd":"hook","event":"SessionStart","pid":4036,
 "session_id":"s1","source":"startup"}`, state("Idle"),
          ["WatchTranscript { process, session: s1, cwd }", "AppendHistory(Shown { Unknown → Idle })", "PublishState"]),
        step("An effect creates an owned task", "effects", ["core", "effects", "watcher"],
          "The reducer requests a watcher; the effect runner performs the I/O. Its HashMap<ProcKey, JoinHandle<()>> has at most one stored watcher per process. A new entry aborts the previous handle.",
          "Rust · watcher configuration", `WatchSpec {
    path: transcript_path(&layout.claude_projects, &cwd, &session),
    process, session, account: default_account(),
    poll: Duration::from_millis(200),
    replay_bytes: 128 * 1024,
}`, state("Idle"), [],
          "The current path is derived from Claude's projects directory, cwd and session. Taking the explicit transcript path from the source is a proposed redesign."),
        step("Tail bytes become inputs", "watcher", ["watcher", "adapters", "inbox", "core", "board", "fold", "view"],
          "Tail retains its byte cursor and incomplete line. Parsed JSON lines enter claude::line, which returns zero or more Inputs. The first successful poll is inferred; subsequent polls are confirmed.",
          "Illustrative transcript line", `{"type":"user","promptId":"p1",
 "message":{"content":"Explain this module"}}`, state("Working", { turn: "Turn · p1" }),
          ["AppendHistory(Shown { Idle → Working })", "PublishState"]),
        step("/clear changes session identity", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold", "view"],
          "SessionStarted for s2 is allowed to rotate the conversation. It clears activity, prompts, subagents, waits and compaction, then requests the new transcript. ProcKey remains unchanged.",
          "Rust · normalized event kind", `EventKind::SessionStarted {
    source: StartSource::Clear,
    title: None,
}
// Event.session = "s2"`, state("Idle"), ["WatchTranscript { process, session: s2, cwd }", "AppendHistory(Shown { Working → Idle })", "PublishState"]),
        step("Swap watchers; reject a late old event", "effects", ["core", "effects", "watcher", "adapters", "inbox", "core", "board"],
          "The runner aborts s1's watcher and starts s2's. If an s1 event was already queued, the reducer's current-session check rejects it. Cancellation alone is not the correctness boundary.",
          "Rust · session guard", `Some(current) if *current != event.session && !starting
    => return effects.finish(),`, state("Idle"), [],
          "Known gap: if the /clear hook is lost, current discovery only fills a missing session; it does not repair an already populated, stale session from a later sweep.")
      ]
    },
    {
      id: "snapshots", title: "A late snapshot must not undo an event", subtitle: "AsOf → freshness check → whole-scope replacement",
      status: "implemented",
      summary: "An executable reducer capability demonstrated with injected Subagents snapshots. The golden has the model and tests for a panel, but no live subagent-panel adapter/driver. This is a model trace, not a currently connected data source.",
      source: refs(["src/model.rs", 486], ["src/reduce/snapshot.rs", 73], ["src/reduce/snapshot.rs", 197], ["src/reduce/event.rs", 180], ["tests/t09_snapshot_tick.rs", 179]),
      steps: [
        step("An event creates a child row", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold"],
          "At t3, SubagentStarted creates child a1 in EventsOnly mode. The parent is Idle itself, but a running child derives Delegating. Child rows are separate from the parent's tools map.",
          "Rust · event at t3", `EventKind::SubagentStarted {
    subagent: "a1".into(), kind: SubagentKind::Task,
}`, state("Delegating", { children: "1 · a1 Running" }), ["AppendHistory(Shown { Idle → Delegating })", "PublishState"]),
        step("A panel snapshot is taken, then delayed", "adapters", ["adapters", "inbox"],
          "At t4, an illustrative panel reports a1 as Running. The snapshot's taken time describes when the observation was made, not when the core eventually receives it.",
          "Rust · injected snapshot", `Snapshot {
    taken: t4,
    kind: SnapshotKind::Subagents {
        session: "s1".into(),
        rows: vec![SubagentRow {
            subagent: "a1".into(),
            status: SubagentStatus::Running, ..
        }],
    },
}`, state("Delegating", { children: "1 · a1 Running" }), [], "No current daemon task produces this panel snapshot. Tests can inject it directly."),
        step("A newer event closes the child first", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold", "view"],
          "At t5, SubagentEnded arrives before the delayed panel. EventsOnly mode removes the row. agent.last_event is now t5 and the parent returns to Idle.",
          "Rust · event at t5", `EventKind::SubagentEnded {
    subagent: "a1".into(),
    outcome: SubagentOutcome::Completed,
}`, state("Idle"), ["AppendHistory(Shown { Delegating → Idle })", "PublishState"]),
        step("Reject the stale panel", "core", ["inbox", "core", "board"],
          "When the t4 snapshot arrives, AsOf::supersedes(t5) is false. Nothing is replaced, so the snapshot cannot bring a1 back. Equal timestamps also fail the strict freshness check.",
          "Rust · actual freshness rule", `match last_event {
    Some(e) => self.as_of > e,
    None => true,
}
// t4 > t5 is false`, state("Idle"), [],
          "This check uses the agent-wide last_event, not a separate clock for each child. It is not applied uniformly to every SnapshotKind."),
        step("A fresh panel takes ownership of the table", "board", ["adapters", "inbox", "core", "board", "fold", "view"],
          "At t7, a newer panel containing child b1 is accepted. It replaces the child table wholesale and changes SubagentSource to Panel. Existing gates survive only on rows still present.",
          "Rust · replacement result", `Subagents {
    rows: { "b1" => Subagent { status: Running, .. } },
    as_of: Some(t7),
    source: SubagentSource::Panel,
}`, state("Delegating", { children: "1 · b1 Running" }), ["AppendHistory(Shown { Idle → Delegating })", "PublishState"]),
        step("Events refine a panel-owned row", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold", "view"],
          "At t8, SubagentEnded marks b1 Completed and clears its gate. In Panel mode the event leaves the row in place: the next snapshot decides membership.",
          "Rust · panel-mode event branch", `row.status = SubagentStatus::Completed;
row.gate = None;
// row remains in agent.subagents.rows`, state("Idle", { children: "1 · b1 Completed" }), ["AppendHistory(Shown { Delegating → Idle })", "PublishState"]),
        step("Absence in the fresh panel removes it", "board", ["adapters", "inbox", "core", "board"],
          "At t9, an empty panel replaces the whole scope. The completed row disappears. The reducer requests publication, but the current View does not include completed child rows, so the equality gate can suppress this output.",
          "Rust · fresh empty snapshot", `Snapshot {
    taken: t9,
    kind: SnapshotKind::Subagents {
        session: "s1".into(), rows: vec![],
    },
}`, state("Idle"), ["PublishState · may render equal and produce no watch update"])
      ]
    },
    {
      id: "deadlines", title: "Quiet is not dead", subtitle: "Stale-turn deadline → gate protection → process death cleanup",
      status: "implemented",
      summary: "A silent turn and a dead process are different facts. The timer may retire an unguarded stale turn; only Died removes the agent. This trace deliberately leaves a joined approval open while time passes.",
      source: refs(["src/reduce/tick.rs", 13], ["src/reduce/tick.rs", 67], ["src/daemon/scanner.rs", 54], ["src/reduce/event.rs", 28], ["src/daemon/effects.rs", 74], ["docs/audit/repro/adv_daemon_bugs.rs", 30]),
      steps: [
        step("A turn has a stale deadline", "core", ["board", "core"],
          "A Turn with no blocked own tool and no compaction has a deadline ten minutes after its last tool/turn activity update. CPU ticks do not themselves refresh that activity deadline.",
          "Rust · deadline", `Some(last_event + TURN_STALE)
// TURN_STALE = Duration::from_secs(10 * 60)`, state("Working", { turn: "Turn · quiet", tools: "1 · Running" })),
        step("A joined approval protects the turn", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold"],
          "PermissionAsked blocks the tracked tool. stale_turn_deadline now returns None for this turn: a permission prompt can be perfectly live while the process and transcript are quiet.",
          "Rust · stale-turn exclusion", `let gated = tools.values()
    .any(|t| matches!(t.gate, Gate::Blocked { .. }));
(!gated && !compacting).then(|| *last_event + TURN_STALE)`, state("Attention", { turn: "Turn · protected", tools: "1", gates: "1 · joined Human gate" }), ["AppendHistory(Shown { Working → Attention })", "PublishState"]),
        step("Ten minutes of silence does not clear red", "fold", ["core", "board", "fold"],
          "Even if tick is called because another agent has a deadline, this joined gate survives. Unjoined prompts have a separate 30-minute leak bound; they are not what this example uses.",
          "Rust · outcome of an illustrative tick", `reduce::tick(&mut board, ten_minutes_later);
// joined Gate::Blocked remains
// agent.phase(limits) == Phase::Attention`, state("Attention", { turn: "Turn · protected", tools: "1", gates: "1 · joined Human gate" })),
        step("The next sweep no longer sees the process", "scanner", ["scanner", "inbox"],
          "The scanner first sends the current Processes snapshot, then emits a Died event for each previously known ProcKey now absent. An empty Processes snapshot alone does not remove agents in the reducer.",
          "Rust · scanner output sequence", `Input::Snapshot(Snapshot {
    taken: now, kind: SnapshotKind::Processes(vec![]),
});
Input::Event(Event {
    process: Some(key), session: "".into(),
    kind: EventKind::Died, ..
});`, state("Attention", { turn: "Turn · protected", tools: "1", gates: "1 · joined Human gate" }), [],
          "Known bug: proc::sweep returns an empty Vec on a top-level read error, so an unreadable sweep can incorrectly generate these deaths. Error and empty need distinct representations."),
        step("Died removes the whole agent", "board", ["inbox", "core", "board", "view"],
          "Died is handled before the session mismatch guard. Board.agents.remove removes the process and all its nested state; the chip is absent rather than painted as a special dead phase.",
          "Rust · removal branch", `if let Some(gone) = board.agents.remove(&key) {
    effects.push(Effect::Forget { process: key });
    effects.push(Effect::AppendHistory(HistoryEntry::AgentGone {
        process: key, session: gone.session,
    }));
}`, state("Absent", { turn: "Agent removed", shown: "Absent" }), ["Forget { process }", "AppendHistory(AgentGone)", "PublishState"]),
        step("Cleanup runs outside the core", "effects", ["core", "effects", "watcher"],
          "The effect runner removes and aborts the process's watcher. It forwards AgentGone to the best-effort history queue. The core can continue processing other inputs while the history appender performs disk I/O.",
          "Rust · watcher cancellation", `if let Some(task) = self.tasks.remove(process) {
    task.abort();
}`, state("Absent", { turn: "Agent removed", shown: "Absent" }), ["Watcher aborted", "AgentGone queued for history"]),
        step("History records the lifecycle edge", "history", ["effects", "history"],
          "The appender stores a lifecycle record separately from the latest View. History and output snapshots serve different purposes: what happened versus what is currently shown.",
          "Rust · history value", `HistoryEntry::AgentGone {
    process: ProcKey { pid: 4036, start: 91000 },
    session: Some("s1".into()),
}`, state("Absent", { turn: "Agent removed", shown: "Absent" }), [],
          "Current Board has no dead-key tombstones. A later stale Processes input can recreate the key; normal scanner ordering prevents that particular order, but the reducer alone does not.")
      ]
    },
    {
      id: "slow-reader", title: "A slow reader cannot stall the board", subtitle: "Arc<View> → latest-value watch → independent output tasks",
      status: "implemented",
      summary: "The view channel retains the latest value, not a queue of every frame. Each subscriber owns its socket task. The file publisher independently coalesces updates for 250 ms. All output numbers below are illustrative.",
      source: refs(["src/daemon/core.rs", 61], ["src/daemon/rpc.rs", 94], ["src/daemon/publish.rs", 17], ["src/view.rs", 66], ["src/daemon/mod.rs", 59]),
      steps: [
        step("Subscribe asks for the current view", "socket", ["subscriber", "socket"],
          "A client writes one newline-delimited command. The handler immediately serializes the current borrowed View, then waits for changes. The current API also supports a one-shot list request.",
          "Wire · client request", `{"cmd":"subscribe"}\n`, state("Idle")),
        step("The response is a full snapshot", "subscriber", ["view", "subscriber"],
          "The RPC wrapper is snapshot; the inner document is switchboard-rust/1. Instants remain inside View and are converted to wall-clock milliseconds only when serializing.",
          "Illustrative full RPC response", `{"snapshot":{"schema":"switchboard-rust/1","updated_ms":1000000,
 "agents":[{"pid":4036,"start":91000,"session_id":"s1",
 "name":"demo","cwd":"/work/demo","phase":"idle","lamp":"green",
 "since_ms":990000,"truth":"idle","reason":"idle","focused":false}]}}`, state("Idle")),
        step("The client stops reading", "subscriber", ["view", "subscriber"],
          "Once its socket buffer fills, this subscriber's write_all awaits capacity. That task is separate from the core. It is not holding a Board lock or blocking the reducer.",
          "Rust · this connection waits here", `write.write_all(line.as_bytes()).await?;
// Core is another task and owns Board.`, state("Idle"), [],
          "There is currently no write timeout. The stalled subscriber can retain its own task and a frame, but not an unbounded per-subscriber queue of all Views."),
        step("The core keeps replacing the latest view", "view", ["inbox", "core", "board", "fold", "view"],
          "As fresh input arrives, the core renders a View and compares it with the previous one. Different views replace the Arc in watch synchronously; equal renders do not wake readers.",
          "Rust · equality gate", `view.send_if_modified(|current| {
    if **current == next { return false; }
    *current = Arc::new(next);
    true
});`, state("Working", { turn: "Turn" }), ["PublishState handled by the core"]),
        step("Another change supersedes the first", "view", ["inbox", "core", "board", "fold", "view"],
          "Suppose a confirmed prompt moves Working to Attention before the slow client catches up. watch now holds the Attention view. It does not retain a required delivery slot for every intermediate Working frame.",
          "Rust · channel shape", `watch::Sender<Arc<View>>
// latest View: truth = Attention, shown = Attention`, state("Attention", { turn: "Turn", tools: "1", gates: "1 · Human" }), ["AppendHistory(Shown)", "PublishState"]),
        step("The file publisher coalesces independently", "publisher", ["view", "publisher"],
          "The publisher waits 250 ms after observing a change, borrows the newest View, and writes a sibling temporary file before renaming it over state.json. Readers see the old or new complete document.",
          "Rust · publication sequence", `view.changed().await?;
tokio::time::sleep(coalesce).await;
let latest = Arc::clone(&view.borrow_and_update());
write_atomic(&path, body.as_bytes()).await;`, state("Attention", { turn: "Turn", tools: "1", gates: "1 · Human" }), ["state.json replaced atomically"]),
        step("The slow client catches up to the newest view", "subscriber", ["view", "subscriber"],
          "When the old blocked write completes, changed() notices unseen updates. The next loop borrows the latest Attention view. It may skip intermediate views, while the separate history path has attempted to record displayed transitions.",
          "JSON · selected current agent fields", `{"phase":"attention","lamp":"red","truth":"attention",
 "reason":"wants approval to run Bash"}`, state("Attention", { turn: "Turn", tools: "1", gates: "1 · Human" }), [],
          "The wire carries whole snapshots, including reason text. There is no lazy tooltip endpoint, delta protocol or seqlock output in this implementation.")
      ]
    },
    {
      id: "replay-gap", title: "Restart replays history as fresh inputs", subtitle: "Known gaps · missing gates, synthetic history, stale account limits",
      status: "known-gap",
      summary: "This is the current recovery path, including audited failures. There is no Input::Replay batch, persisted gate model, or dual-clock Now type yet. The diagram intentionally exposes what reconstruction can and cannot know.",
      source: refs(["src/daemon/watcher.rs", 39], ["src/ingest/claude/transcript.rs", 56], ["src/ingest/claude/transcript.rs", 186], ["src/reduce/tick.rs", 83], ["docs/audit/repro/adv_daemon_bugs.rs", 140], ["docs/audit/repro/adv_bugs.rs", 58]),
      steps: [
        step("Restart begins with an empty board", "board", ["scanner", "inbox", "core", "board", "view"],
          "Discovery reconstructs process and session identity. The agent begins Unobserved/Unknown; the reducer does not reload open gates from history. The process itself may still be parked at a real approval prompt.",
          "Rust · reconstructed initial activity", `Activity::Unobserved { since: now }
// prompts = Prompts::default()
// shown = Shown::new(Phase::Unknown, now)`, state("Unknown", { turn: "Unobserved" }), ["AppendHistory(AgentFound)", "WatchTranscript", "PublishState"]),
        step("Read the last 128 KiB", "watcher", ["core", "effects", "watcher"],
          "The watcher opens near the file's end and preserves complete lines. It processes only the bounded tail, not a full reconstruction of the session. All events in its initial poll use Inferred confidence and the current local receipt time.",
          "Rust · initial watcher context", `LineCtx {
    process, session, account,
    now: now(),
    confidence: Confidence::Inferred,
}`, state("Unknown", { turn: "Unobserved" })),
        step("Old lines become ordinary events", "adapters", ["watcher", "adapters", "inbox", "core", "board", "fold"],
          "A historical typed prompt, assistant end_turn, then another prompt are applied separately. Changes inside the calm severity band are immediate even when inferred, so shown can move Working → Idle → Working during one tail read.",
          "Illustrative adapted event sequence", `TurnStarted { turn: Some(Prompt("old-1")), origin: Human }
TurnEnded { turn: None, outcome: Completed }
TurnStarted { turn: Some(Prompt("old-2")), origin: Human }
// These abbreviate real EventKind variants.`, state("Working", { turn: "Turn · old-2" }), ["AppendHistory(Shown) for each visible phase change", "PublishState for changed inputs"]),
        step("Recovery writes synthetic transition history", "history", ["core", "effects", "history"],
          "There is no replay flag in HistoryEntry or a batch-level settle. Each ordinary apply can append Shown, so restarting adds transitions for work that happened long ago. Output coalescing does not suppress these history effects.",
          "Rust · current history shape", `HistoryEntry::Shown {
    process, session,
    from: Phase::Working, to: Phase::Idle,
    held: now.saturating_duration_since(shown_since),
}`, state("Working", { turn: "Turn · old-2" }), ["Historical work appears as new Shown edges"],
          "Audit repro expects at most one replay transition; current code emits several. A replay batch is a proposal, not an existing Input variant."),
        step("A real open gate is absent from the transcript", "board", ["watcher", "adapters", "inbox", "core", "board", "fold", "view"],
          "The tail may end with a tool_use whose approval is still open. That line reconstructs ToolDispatched and Gate::Running, but no PermissionRequest. The chip can become Working/green while the real UI needs a person.",
          "Rust · reconstructed tool", `Tool {
    name: "Bash".into(),
    gate: Gate::Running, ..
}
// Real external dialog: still waiting for approval.`, state("Working", { turn: "Turn", tools: "1 · reconstructed Running", gates: "0 · actual prompt unknown" }), ["PublishState"],
          "Live status-file reconciliation is being investigated. Current golden does not ingest Claude's status/waitingFor fields to close this gap."),
        step("An old limit line affects the entire account", "adapters", ["watcher", "adapters", "inbox", "core", "board", "fold"],
          "In another tail, an API error carries quotaLimits. The adapter emits both a failed turn and a Limits snapshot. A reset time months in the past is still stored as Rejected; all agents currently use account default.",
          "Illustrative historical JSON", `{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit",
 "quotaLimits":{"status":"rejected","resetsAt":1770000000,
 "rateLimitType":"five_hour"}}`, state("Blocked", { shown: "Working", turn: "Idle · after Failed" }), ["SnapshotKind::Limits { account: default, value: Rejected }", "PublishState"],
          "This step switches to a second example tail to expose the account-wide limit bug. It is not a continuation of the unresolved prompt's real UI state."),
        step("Time does not expire that account limit", "fold", ["core", "board", "fold", "view"],
          "After the pending dwell, shown becomes Blocked. Later ticks still see LimitStatus::Rejected: next_deadline has no account reset deadline and tick receives only Instant. Passing the wall-clock resets_at value does not clear it.",
          "Rust · current phase rule", `if limits.is_some_and(|l| l.status == LimitStatus::Rejected) {
    return Phase::Blocked;
}
// No resets_at check here or in tick.`, state("Blocked", { turn: "Idle · after Failed" }), ["AppendHistory(Shown { Working → Blocked })", "PublishState"],
          "The current design needs both recovery semantics and a real limit-expiry rule. This trace demonstrates the bug; it does not prescribe the unresolved redesign.")
      ]
    },
    {
      id: "subagent-gap", title: "A child has two calls, but only one gate slot", subtitle: "Known gap · an unrelated completion clears a live prompt",
      status: "known-gap",
      summary: "The parent has a tools map, but Subagent currently has only Option<Gate>. This asymmetry explains the audited bug without requiring provider-specific logic in the fold. A shared per-call actor model remains a proposal.",
      source: refs(["src/model.rs", 878], ["src/reduce/event.rs", 131], ["src/reduce/event.rs", 368], ["src/reduce/event.rs", 491], ["docs/audit/repro/adv_bugs.rs", 267]),
      steps: [
        step("The child starts beneath an idle parent", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold"],
          "SubagentStart creates a1 in the parent's Subagents table. With no own turn open, the running child makes the parent Delegating.",
          "Rust · child row", `Subagent {
    kind: SubagentKind::Task,
    status: SubagentStatus::Running,
    gate: None, ..
}`, state("Delegating", { children: "1 · a1 Running" }), ["AppendHistory(Shown { Idle → Delegating })", "PublishState"]),
        step("Two child tool calls are in flight", "adapters", ["hook", "socket", "adapters", "inbox", "core", "board"],
          "The adapter marks tool events Actor::Subagent(a1). Unlike the main actor, the current subagent dispatch branch does not store a ToolUseId → Tool map; it refreshes last_seen and may join a parked prompt.",
          "Rust · two normalized events", `Actor::Subagent("a1".into())
ToolDispatched { call: "bash-1".into(), tool: "Bash".into(), hash }
ToolDispatched { call: "read-2".into(), tool: "Read".into(), hash }
// No per-call map is retained on Subagent.`, state("Delegating", { children: "1 · a1 / 2 external calls", tools: "0 · parent only" }), ["PublishState requested; View may be unchanged"]),
        step("Bash asks for a human answer", "board", ["hook", "socket", "adapters", "inbox", "core", "board", "fold"],
          "A PermissionRequest from a1 writes Gate::Blocked onto the child row. The reducer discards call-level ownership at this storage boundary: the row can remember one gate, but not which of its calls holds it.",
          "Rust · stored child gate", `row.gate = Some(Gate::Blocked {
    owner: Ownership::Human,
    kind: GateKind::Approval,
    since: now, req: None,
});`, state("Attention", { children: "1 · a1 Running", gates: "1 · child a1 / Bash", tools: "0 · parent" }), ["AppendHistory(Shown { Delegating → Attention })", "PublishState"]),
        step("The parent's fold sees the child's gate", "view", ["board", "fold", "view", "subscriber"],
          "Agent::wants_a_human checks own tools, unjoined prompts and child gates. Therefore the parent chip correctly turns red even though its own activity remains Idle.",
          "Rust · fold includes descendants", `let subagents = self.subagents.rows.values().any(|s| {
    matches!(s.gate, Some(Gate::Blocked {
        owner: Ownership::Human, ..
    }))
});`, state("Attention", { children: "1 · a1 Running", gates: "1 · child a1 / Bash" }), [],
          "Separate known gap: view::human_gate currently omits stored child-row gates, so tooltip text can say subagent running while the chip is red."),
        step("The unrelated Read call completes", "board", ["hook", "socket", "adapters", "inbox", "core", "board"],
          "PostToolUse for read-2 becomes ToolSettled with a1 as actor. The child branch calls lift_subagent(agent, id), ignoring the call ID. That function takes the child row's gate unconditionally.",
          "Rust · current ToolSettled branch", `Actor::Subagent(id) => lift_subagent(agent, id)

// Inside lift_subagent:
agent.subagents.rows.get_mut(id)
    .and_then(|row| row.gate.take());`, state("Delegating", { shown: "Attention", children: "1 · a1 Running", gates: "0 · actual Bash prompt still open" }), [],
          "This pause is inside one synchronous reducer invocation; no other input interleaves before settle."),
        step("False green reaches the output", "subscriber", ["board", "fold", "view", "subscriber"],
          "With no stored gate, the fold derives Delegating again. De-escalation is immediate, so the red chip becomes green although the Bash approval still exists outside the daemon.",
          "JSON · selected actual output fields", `{"phase":"delegating","lamp":"green","truth":"delegating",
 "reason":"1 subagent(s) running"}`, state("Delegating", { children: "1 · a1 Running", gates: "0 · actual Bash prompt still open" }), ["AppendHistory(Shown { Attention → Delegating })", "PublishState"],
          "The missing information is the per-call association. The audit repro pins the desired behavior: settling read-2 must leave bash-1's prompt open.")
      ]
    }
  ];
})();
