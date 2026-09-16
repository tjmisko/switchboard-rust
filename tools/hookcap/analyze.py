#!/usr/bin/env python3
"""Answer the open questions from a hookcap recording.

  analyze.py                 every recorded day
  analyze.py 2026-09-13 ...  only those days

Each section prints its evidence and a one-line reading. A section with no data
says so rather than guessing: an event that was never exercised is not an event
that does not fire. The manual scenarios in PROTOCOL.md exist to exercise them.
"""

import json
import os
import sys
from collections import Counter, defaultdict
from datetime import datetime
from pathlib import Path

ROOT = Path(os.environ.get("HOOKCAP_DIR", Path.home() / ".local/state/switchboard/hookcap"))
INTERRUPT_PREFIX = "[Request interrupted by user"


# ---------------------------------------------------------------------------
# loading

def read_jsonl(path):
    if not path.exists():
        return []
    rows = []
    for line in path.open():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def load(days):
    dirs = [ROOT / d for d in days] if days else sorted(p for p in ROOT.iterdir() if p.is_dir())
    data = defaultdict(list)
    for d in dirs:
        for stream in ["hooks", "statusline", "subagents", "sampler"]:
            data[stream] += read_jsonl(d / f"{stream}.jsonl")
    for rows in data.values():
        rows.sort(key=lambda r: r.get("ts", 0))
    return data


def iso(ts):
    try:
        return datetime.fromisoformat(ts.replace("Z", "+00:00")).timestamp()
    except (AttributeError, ValueError):
        return None


def pct(values, q):
    if not values:
        return None
    values = sorted(values)
    return values[min(len(values) - 1, int(q * len(values)))]


def ms(seconds):
    return "—" if seconds is None else f"{seconds * 1000:.0f} ms"


def section(title):
    print(f"\n## {title}")


def reading(text):
    print(f"   → {text}")


def hooks_of(data, provider=None, event=None):
    return [h for h in data["hooks"] if h.get("kind") == "hook"
            and (provider is None or h.get("provider") == provider)
            and (event is None or h.get("event") == event)]


def sid(h):
    p = h.get("payload")
    return p.get("session_id") if isinstance(p, dict) else None


def pid(h):
    return (h.get("agent") or {}).get("pid")


def next_hook(data, after, session, events, within=3600):
    for h in hooks_of(data):
        if h["ts"] <= after or h["ts"] > after + within:
            continue
        if sid(h) == session and h.get("event") in events:
            return h
    return None


def scope(h):
    p = h.get("payload") or {}
    return h.get("provider"), sid(h), p.get("agent_id"), p.get("turn_id")


def permission_pairs(data, provider):
    """Nearest unused matching call in the same session, child and turn.

    Hashes identify input, not an invocation: repeated identical commands remain
    a temporal inference. Missing hashes never match each other.
    """
    pre = hooks_of(data, provider, "PreToolUse")
    used = set()
    pairs = []
    for ask in sorted(hooks_of(data, provider, "PermissionRequest"), key=lambda h: h["ts"]):
        p = ask.get("payload") or {}
        candidates = []
        for h in pre:
            hp = h.get("payload") or {}
            if id(h) in used or not sid(ask) or scope(h) != scope(ask) or hp.get("tool_name") != p.get("tool_name"):
                continue
            if abs(h["ts"] - ask["ts"]) > (3600 if provider == "codex" else 10):
                continue
            if p.get("tool_use_id"):
                match = hp.get("tool_use_id") == p["tool_use_id"]
            else:
                key = "command_hash" if provider == "codex" and ask.get("command_hash") else "input_hash"
                match = bool(ask.get(key)) and ask[key] == h.get(key)
            if match:
                candidates.append(h)
        twin = min(candidates, key=lambda h: abs(h["ts"] - ask["ts"]), default=None)
        if twin is not None:
            used.add(id(twin))
        pairs.append((ask, twin, len(candidates)))
    return pairs


def tool_completion(data, pre, after):
    if pre is None:
        return None
    call_id = (pre.get("payload") or {}).get("tool_use_id")
    if not call_id:
        return None
    return min((h for h in hooks_of(data, pre["provider"])
                if h["ts"] > after and scope(h) == scope(pre)
                and h["event"] in {"PostToolUse", "PostToolUseFailure"}
                and (h.get("payload") or {}).get("tool_use_id") == call_id),
               key=lambda h: h["ts"], default=None)


# ---------------------------------------------------------------------------
# questions

def events_seen(data):
    section("Which hook events fired at all")
    counts = Counter((h.get("provider"), h.get("event")) for h in hooks_of(data))
    for (provider, event), n in sorted(counts.items()):
        print(f"   {provider:7} {event:22} {n:6}")
    if not counts:
        reading("no hook records")


def status_file_tracks_prompts(data):
    section("Does sessions/<pid>.json status=waiting track prompts, by tool?")
    changes = [r for r in data["sampler"] if r.get("kind") == "session_file"]
    statuses = Counter((r["file"].get("status"), r["file"].get("waitingFor")) for r in changes if "status" in r.get("changed", {}))
    print("   status transitions seen (status, waitingFor):")
    for (status, waiting_for), n in statuses.most_common():
        print(f"     {str(status):8} {str(waiting_for):24} {n}")

    pairs = permission_pairs(data, "claude")
    paired = {id(pre) for _, pre, _ in pairs if pre is not None}
    prompts = [(ask, pre) for ask, pre, _ in pairs]
    prompts += [(h, h) for h in hooks_of(data, "claude", "PreToolUse") if id(h) not in paired
                and (h.get("payload") or {}).get("tool_name") in ("AskUserQuestion", "ExitPlanMode")]
    onset, release, durations, missed = [], [], [], 0
    by_tool = defaultdict(lambda: [0, 0])
    for h, pre in prompts:
        tool = (h.get("payload") or {}).get("tool_name")
        by_tool[tool][0] += 1
        p = pid(h)
        rise = next((r for r in changes if r["pid"] == p and h["ts"] - 1 <= r["ts"] <= h["ts"] + 10
                     and r["file"].get("sessionId") == sid(h)
                     and r["file"].get("status") == "waiting"), None)
        if rise is None:
            missed += 1
            continue
        onset.append(rise["ts"] - h["ts"])
        by_tool[tool][1] += 1
        fall = next((r for r in changes if r["pid"] == p and r["ts"] > rise["ts"]
                     and r["file"].get("sessionId") == sid(h)
                     and r["file"].get("status") != "waiting"), None)
        answer = tool_completion(data, pre, h["ts"])
        if fall:
            durations.append(fall["ts"] - rise["ts"])
        if fall and answer:
            release.append(fall["ts"] - answer["ts"])
    for tool, (count, seen) in sorted(by_tool.items()):
        print(f"   {tool}: {seen}/{count} prompts showed waiting")
    print(f"   prompts: {len(prompts)}   waiting seen within 10 s: {len(onset)}   not seen: {missed}")
    print(f"   onset after hook   p50 {ms(pct(onset, .5))}  p90 {ms(pct(onset, .9))}")
    print(f"   observed waiting durations (s): {[round(v, 3) for v in durations]}")
    print(f"   release vs matching tool completion ({len(release)} pairs)   p50 {ms(pct(release, .5))}  p90 {ms(pct(release, .9))}  (negative = file first)")
    if prompts:
        reading("waiting tracked these observed tool prompts; untested prompt types still need scenarios" if missed == 0 and onset else
                "partial: inspect the prompts that never showed waiting before relying on it")
    else:
        reading("no prompts recorded; run PROTOCOL scenario 1")


def permission_denied_on_decline(data):
    section("Does PermissionDenied fire when a human declines?")
    by_kind = defaultdict(lambda: [0, 0])
    denied_ids = {(h.get("payload") or {}).get("tool_use_id") for h in hooks_of(data, "claude", "PermissionDenied")}
    for path, lo, hi in transcripts(data):
        for line in lines_between(path, lo, hi):
            kind = line.get("toolDenialKind")
            if not kind:
                continue
            ids = [b.get("tool_use_id") for b in content_blocks(line) if b.get("type") == "tool_result"]
            by_kind[kind][0 if any(i in denied_ids for i in ids) else 1] += 1
    for kind, (with_hook, without) in sorted(by_kind.items()):
        print(f"   {kind:22} with hook {with_hook:4}   without {without:4}")
    if "user-rejected" in by_kind:
        with_hook, without = by_kind["user-rejected"]
        reading("yes" if with_hook and not without else "no" if without and not with_hook else "mixed")
    else:
        reading("no human declines recorded; run PROTOCOL scenario 2")


def interrupts(data):
    section("Esc: which interrupts produce a hook, and do they carry interruptedMessageId?")
    rows = []
    for path, lo, hi in transcripts(data):
        session = Path(path).stem
        for line in lines_between(path, lo, hi):
            if line.get("type") != "user":
                continue
            texts = [b.get("text", "") for b in content_blocks(line) if b.get("type") == "text"]
            if not any(t.startswith(INTERRUPT_PREFIX) for t in texts):
                continue
            at = iso(line.get("timestamp"))
            near = [h["event"] for h in hooks_of(data, "claude") if sid(h) == session and at and at - 3 <= h["ts"] <= at + 5]
            rows.append((bool(line.get("interruptedMessageId")), "for tool use" in " ".join(texts), near))
    for has_id, tool, near in rows:
        print(f"   id={str(has_id):5} tool={str(tool):5} hooks nearby: {', '.join(near) or 'none'}")
    if rows:
        no_id = sum(1 for r in rows if not r[0])
        reading(f"{no_id}/{len(rows)} lack interruptedMessageId; "
                f"Stop fired on {sum('Stop' in r[2] for r in rows)}/{len(rows)}")
    else:
        reading("no interrupts recorded; run PROTOCOL scenarios 3–5")


def statusline(data):
    section("statusLine: cadence, and does api_ms move during a turn?")
    by_session = defaultdict(list)
    for r in data["statusline"]:
        by_session[r.get("session_id")].append(r)
    gaps = [b["ts"] - a["ts"] for rows in by_session.values() for a, b in zip(rows, rows[1:])]
    print(f"   records {len(data['statusline'])}   gap p10 {ms(pct(gaps, .1))}  p50 {ms(pct(gaps, .5))}  p90 {ms(pct(gaps, .9))}")
    moved, turns = 0, 0
    for start in hooks_of(data, "claude", "UserPromptSubmit"):
        end = next_hook(data, start["ts"], sid(start), {"Stop", "StopFailure"})
        if not end:
            continue
        inside = [r.get("api_ms") for r in by_session[sid(start)] if start["ts"] < r["ts"] < end["ts"] - 0.5]
        inside = [v for v in inside if isinstance(v, (int, float))]
        if len(inside) >= 2:
            turns += 1
            moved += inside[-1] > inside[0]
    print(f"   turns with ≥2 in-turn samples {turns}   api_ms advanced mid-turn in {moved}")
    shapes = Counter(json.dumps(sorted((r.get("rate_limits") or {}).keys())) for r in data["statusline"])
    print(f"   rate_limits key sets: {dict(shapes)}")
    reading("api_ms is a live in-flight signal" if turns and moved == turns else
            "api_ms only moves at turn end" if turns and moved == 0 else "inconclusive")


def subagent_panel(data):
    section("subagentStatusLine: cadence, types and statuses")
    rows = data["subagents"]
    gaps = [b["ts"] - a["ts"] for a, b in zip(rows, rows[1:]) if a.get("session_id") == b.get("session_id")]
    print(f"   invocations {len(rows)}   gap p50 {ms(pct(gaps, .5))}  p90 {ms(pct(gaps, .9))}")
    tasks = [t for r in rows for t in r.get("tasks", [])]
    print(f"   task types {dict(Counter(t.get('type') for t in tasks))}")
    print(f"   task statuses {dict(Counter(t.get('status') for t in tasks))}")
    marks = [m for m in data["hooks"] if m.get("kind") == "mark" and "panel" in m.get("text", "")]
    for m in marks:
        after = [r for r in rows if m["ts"] < r["ts"] < m["ts"] + 30]
        print(f"   after mark \"{m['text']}\": {len(after)} invocations in 30 s")
    reading("fires with the panel closed" if any("closed" in m["text"] and
            any(m["ts"] < r["ts"] < m["ts"] + 30 for r in rows) for m in marks)
            else "run PROTOCOL scenario 7 with marks to settle panel-closed behaviour")


def stop_payloads(data):
    section("Stop.background_tasks[] and session_crons[] element shapes")
    shapes = Counter()
    for h in hooks_of(data, "claude", "Stop"):
        p = h.get("payload") or {}
        for field in ["background_tasks", "session_crons"]:
            for item in p.get(field) or []:
                shapes[(field, json.dumps(item if not isinstance(item, dict) else
                                          {k: type(v).__name__ for k, v in sorted(item.items())}))] += 1
    for (field, shape), n in shapes.most_common():
        print(f"   {field:17} {n:4}  {shape}")
    if not shapes:
        reading("no background work recorded; run PROTOCOL scenario 8")


def prompt_sources(data):
    section("UserPromptSubmit.source values")
    print(f"   {dict(Counter((h.get('payload') or {}).get('source') for h in hooks_of(data, 'claude', 'UserPromptSubmit')))}")


def subagents(data):
    section("Subagent start/stop events and distinct children")
    for provider in ("claude", "codex"):
        starts = Counter((sid(h), h["payload"].get("agent_id")) for h in hooks_of(data, provider, "SubagentStart"))
        stops = Counter((sid(h), h["payload"].get("agent_id")) for h in hooks_of(data, provider, "SubagentStop"))
        print(f"   {provider}: start events {sum(starts.values())}, distinct children {len(starts)}; "
              f"stop events {sum(stops.values())}, distinct children {len(stops)}, max stops/child {max(stops.values(), default=0)}")
        print(f"     children with starts but no stop {len(starts.keys() - stops.keys())}; "
              f"stops but no captured start {len(stops.keys() - starts.keys())}")
    reading("Codex SubagentStop ends a turn; repeated stops do not establish child exit")
    teammates = hooks_of(data, "claude", "TeammateIdle")
    print(f"   TeammateIdle {len(teammates)}")


def clear_rotation(data):
    section("/clear: does sessions/<pid>.json follow, and is CLAUDE_SESSION_ID stale?")
    changes = [r for r in data["sampler"] if r.get("kind") == "session_file" and "sessionId" in r.get("changed", {})]
    clears = [h for h in hooks_of(data, "claude", "SessionStart") if (h.get("payload") or {}).get("source") == "clear"]
    for h in clears:
        follow = next((r for r in changes if r["pid"] == pid(h) and abs(r["ts"] - h["ts"]) < 30
                       and r["file"].get("sessionId") == sid(h)), None)
        env = (h.get("agent") or {}).get("env_session_id")
        print(f"   clear at {h['ts']:.1f}: file follows {ms(follow['ts'] - h['ts']) if follow else 'no'}"
              f"   environ {'stale' if env and env != sid(h) else 'matches' if env else 'absent'}")
    present = [bool((h.get("agent") or {}).get("env_session_id")) for h in hooks_of(data, "claude")]
    print(f"   CLAUDE_SESSION_ID in agent environ on {sum(present)}/{len(present)} hooks")
    if not clears:
        reading("no /clear recorded; run PROTOCOL scenario 6")


def process_signature(data):
    section("What a waiting agent looks like from /proc (for the restart probe)")
    changes = [r for r in data["sampler"] if r.get("kind") == "session_file"]
    procs = [r for r in data["sampler"] if r.get("kind") == "proc" and r.get("comm") == "claude"]
    status_at = {}
    by_status = defaultdict(lambda: {"cpu": [], "kids": Counter()})
    for r in sorted(changes + procs, key=lambda r: r["ts"]):
        if r["kind"] == "session_file":
            status_at[r["pid"]] = r["file"].get("status")
            continue
        status = status_at.get(r["pid"])
        if status is None:
            continue
        by_status[status]["cpu"].append(r.get("cpu_delta", 0))
        by_status[status]["kids"][" ".join(r.get("children", []))] += 1
    for status, s in by_status.items():
        cpu = s["cpu"]
        print(f"   {status:8} samples {len(cpu):6}  cpu ticks/2s p50 {pct(cpu, .5)} p90 {pct(cpu, .9)}  "
              f"children {s['kids'].most_common(2)}")


def join_order(data):
    section("Claude PreToolUse vs PermissionRequest: matching input and observed order")
    matched, first = 0, Counter()
    asks = hooks_of(data, "claude", "PermissionRequest")
    for ask, twin, _ in permission_pairs(data, "claude"):
        if twin:
            matched += 1
            first["PreToolUse" if twin["ts"] <= ask["ts"] else "PermissionRequest"] += 1
    print(f"   PermissionRequest {len(asks)}   matched a PreToolUse by id/hash {matched}   first: {dict(first)}")
    ids = sum(1 for a in asks if (a.get("payload") or {}).get("tool_use_id"))
    print(f"   PermissionRequest carrying tool_use_id: {ids}/{len(asks)}")


def turn_end_timing(data):
    section("Transcript end_turn vs the Stop hook")
    deltas = []
    for stop in hooks_of(data, "claude", "Stop"):
        path = (stop.get("payload") or {}).get("transcript_path")
        ends = [iso(l.get("timestamp")) for l in lines_between(path, stop["ts"] - 120, stop["ts"] + 5)
                if l.get("type") == "assistant" and (l.get("message") or {}).get("stop_reason") == "end_turn"]
        ends = [e for e in ends if e]
        if ends:
            deltas.append(stop["ts"] - max(ends))
    print(f"   pairs {len(deltas)}   Stop after end_turn p50 {ms(pct(deltas, .5))}  p90 {ms(pct(deltas, .9))}  max {ms(max(deltas) if deltas else None)}")


def codex(data):
    section("Codex: matched permission requests, tool completion, and session lifetime")
    asks = hooks_of(data, "codex", "PermissionRequest")
    pairs = permission_pairs(data, "codex")
    order, completion, repeated = [], [], 0
    for ask, pre, candidates in pairs:
        if pre is None:
            continue
        order.append(ask["ts"] - pre["ts"])
        repeated += candidates > 1
        answer = tool_completion(data, pre, ask["ts"])
        if answer:
            completion.append(answer["ts"] - ask["ts"])
    print(f"   PermissionRequest {len(asks)}; with command_hash {sum(bool(a.get('command_hash')) for a in asks)}; "
          f"with tool_use_id {sum(bool(a['payload'].get('tool_use_id')) for a in asks)}")
    print(f"   matched PreToolUse {len(order)}; unmatched {len(asks) - len(order)}; "
          f"multiple input candidates {repeated} (nearest unused selected)")
    print(f"   PreToolUse first {sum(v >= 0 for v in order)}; PermissionRequest first {sum(v < 0 for v in order)}")
    print(f"   PermissionRequest minus matched PreToolUse: {[ms(v) for v in order]}")
    print(f"   matching PostToolUse {len(completion)}; no matching completion {len(order) - len(completion)}")
    print(f"   request-to-tool-completion (includes execution): {[ms(v) for v in completion]}")
    reading("these hooks do not identify a human-visible prompt or isolate approval latency; use scenario 13 marks. "
            "Missing completion does not prove denial; unrelated activity does not resolve a request")
    reasons = Counter(h["payload"].get("reason") for h in hooks_of(data, "codex", "SessionEnd"))
    print(f"   SessionEnd.reason: {dict(reasons)}")
    for end in hooks_of(data, "codex", "SessionEnd"):
        p = pid(end)
        start = (end.get("agent") or {}).get("start")
        died = min((r["ts"] for r in data["sampler"] if r.get("kind") == "proc_gone"
                    and r.get("pid") == p and r.get("start") == start and r["ts"] >= end["ts"]), default=None)
        print(f"   SessionEnd pid {p}: process {'disappearance sampled ' + ms(died - end['ts']) + ' later' if died else 'exit unknown'}")


def notifications(data):
    section("Notification types and delay after the prompt")
    types = Counter((h.get("payload") or {}).get("notification_type") for h in hooks_of(data, "claude", "Notification"))
    print(f"   {dict(types)}")
    delays = []
    for n in hooks_of(data, "claude", "Notification"):
        if (n.get("payload") or {}).get("notification_type") != "permission_prompt":
            continue
        ask = max((h for h in hooks_of(data, "claude", "PermissionRequest") if sid(h) == sid(n) and h["ts"] < n["ts"]),
                  key=lambda h: h["ts"], default=None)
        if ask:
            delays.append(n["ts"] - ask["ts"])
    print(f"   permission_prompt after PermissionRequest p50 {ms(pct(delays, .5))}")


def headless(data):
    section("Headless -p runs")
    runs = [h for h in hooks_of(data, "claude") if set((h.get("agent") or {}).get("flags", [])) & {"-p", "--print"}]
    print(f"   hooks from -p agents {len(runs)}   events {dict(Counter(h['event'] for h in runs))}")
    kinds = Counter(((h.get("agent") or {}).get("session_file") or {}).get("kind") for h in runs)
    print(f"   session file kind for -p agents: {dict(kinds)}")


def marks(data):
    section("Manual scenario timeline (30 s after each mark)")
    for m in (r for r in data["hooks"] if r.get("kind") == "mark"):
        after = [f"{h['provider']}:{h['event']}" for h in hooks_of(data) if m["ts"] < h["ts"] < m["ts"] + 30]
        files = [f"{r['file'].get('status')}/{r['file'].get('waitingFor')}" for r in data["sampler"]
                 if r.get("kind") == "session_file" and m["ts"] < r["ts"] < m["ts"] + 30]
        print(f"   \"{m['text']}\"\n     hooks: {' → '.join(after) or 'none'}\n     status file: {' → '.join(files) or 'no change'}")


# ---------------------------------------------------------------------------
# transcripts

def transcripts(data):
    spans = {}
    for h in hooks_of(data, "claude"):
        path = (h.get("payload") or {}).get("transcript_path")
        if path and not str(path).startswith("<str"):
            lo, hi = spans.get(path, (h["ts"], h["ts"]))
            spans[path] = (min(lo, h["ts"]), max(hi, h["ts"]))
    return [(p, lo - 60, hi + 60) for p, (lo, hi) in spans.items()]


_cache = {}


def lines_between(path, lo, hi):
    if not path:
        return []
    if path not in _cache:
        rows = []
        try:
            for line in open(path, errors="replace"):
                try:
                    v = json.loads(line)
                except ValueError:
                    continue
                v["_ts"] = iso(v.get("timestamp"))
                rows.append(v)
        except OSError:
            pass
        _cache[path] = rows
    return [v for v in _cache[path] if v["_ts"] and lo <= v["_ts"] <= hi]


def content_blocks(line):
    content = (line.get("message") or {}).get("content")
    return [b for b in content if isinstance(b, dict)] if isinstance(content, list) else []


def main():
    data = load(sys.argv[1:])
    if not any(data.values()):
        sys.exit(f"no recordings under {ROOT}")
    for question in [events_seen, status_file_tracks_prompts, permission_denied_on_decline, interrupts,
                     statusline, subagent_panel, stop_payloads, prompt_sources, subagents, clear_rotation,
                     process_signature, join_order, turn_end_timing, codex, notifications, headless, marks]:
        try:
            question(data)
        except Exception as e:  # one broken section must not hide the others
            print(f"   (section failed: {e!r})")


if __name__ == "__main__":
    main()
