#!/usr/bin/env python3
"""Sample agent processes and Claude session files for the capture experiment.

Two loops in one thread:
  every 250 ms  re-read ~/.claude/sessions/<pid>.json for each live agent and
                log any change to its status fields (status, waitingFor, ...)
  every 2 s     scan /proc for claude and codex processes: run state, CPU
                ticks since the last sample, and child process names

Hooks say when an edge happened. This says what the process and Claude's own
status file looked like around it, which is what the restart probe and the
"does status=waiting track open prompts" question need.
"""

import json
import os
import signal
import time
from pathlib import Path

ROOT = Path(os.environ.get("HOOKCAP_DIR", Path.home() / ".local/state/switchboard/hookcap"))
SESSIONS = Path.home() / ".claude/sessions"
FILE_EVERY = 0.25
PROC_EVERY = 2.0
AGENT_COMMS = {"claude", "codex"}
SESSION_FIELDS = ("sessionId", "kind", "status", "waitingFor", "tempo", "state", "needs",
                  "detail", "name", "nameSource", "statusUpdatedAt", "procStart")
CODEX_VERBS = {"exec", "app-server", "mcp", "mcp-server", "login", "resume", "apply", "proto"}

running = True


def stop(*_):
    global running
    running = False


def out_path():
    day = ROOT / time.strftime("%Y-%m-%d")
    day.mkdir(parents=True, exist_ok=True)
    return day / "sampler.jsonl"


def emit(record):
    record["ts"] = time.time()
    with out_path().open("a") as f:
        f.write(json.dumps(record, separators=(",", ":")) + "\n")


def read_stat(pid):
    try:
        text = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return None
    comm = text[text.index("(") + 1:text.rindex(")")]
    fields = text[text.rindex(")") + 2:].split()
    return {"comm": comm, "state": fields[0], "ppid": int(fields[1]),
            "cpu": int(fields[11]) + int(fields[12]), "start": int(fields[19])}


def children(pid):
    try:
        kids = Path(f"/proc/{pid}/task/{pid}/children").read_text().split()
    except OSError:
        return []
    names = []
    for kid in kids:
        try:
            names.append(Path(f"/proc/{kid}/comm").read_text().strip())
        except OSError:
            pass
    return sorted(names)


def argv_shape(pid):
    """Flags and known verbs only: a bare argument may be a prompt."""
    try:
        argv = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
    except OSError:
        return []
    shape = []
    for arg in argv[1:]:
        a = arg.decode(errors="replace")
        if not a:
            continue
        shape.append(a if a.startswith("-") or a in CODEX_VERBS else "<arg>")
    return shape


def argv0(pid):
    try:
        return os.path.basename(Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")[0].decode(errors="replace"))
    except (OSError, IndexError):
        return ""


def scan():
    """Agent processes only. Codex runs each sandboxed command as the same binary
    under argv[0] "codex-linux-sandbox", so comm alone cannot tell them apart."""
    agents = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        st = read_stat(int(entry))
        if st and st["comm"] in AGENT_COMMS and argv0(int(entry)) == st["comm"]:
            agents[(int(entry), st["start"])] = st
    return agents


def session_file(pid):
    try:
        data = json.loads((SESSIONS / f"{pid}.json").read_text())
    except (OSError, ValueError):
        return None
    return {k: data.get(k) for k in SESSION_FIELDS}


def main():
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    emit({"kind": "sampler_start", "file_every_ms": FILE_EVERY * 1000, "proc_every_ms": PROC_EVERY * 1000})

    known = {}      # (pid, start) -> last stat
    files = {}      # pid -> last session-file subset
    next_proc = 0.0
    while running:
        now = time.monotonic()
        if now >= next_proc:
            next_proc = now + PROC_EVERY
            current = scan()
            for key, st in current.items():
                pid, start = key
                last = known.get(key)
                if last is None:
                    emit({"kind": "proc_new", "pid": pid, "start": start, "comm": st["comm"],
                          "ppid": st["ppid"], "argv": argv_shape(pid)})
                emit({"kind": "proc", "pid": pid, "start": start, "comm": st["comm"], "state": st["state"],
                      "cpu_delta": st["cpu"] - (last or st)["cpu"], "children": children(pid)})
            for key in known.keys() - current.keys():
                emit({"kind": "proc_gone", "pid": key[0], "start": key[1]})
                files.pop(key[0], None)
            known = current

        for pid, _ in known:
            subset = session_file(pid)
            if subset is None or subset == files.get(pid):
                continue
            before = files.get(pid) or {}
            changed = {k: [before.get(k), v] for k, v in subset.items() if before.get(k) != v}
            files[pid] = subset
            emit({"kind": "session_file", "pid": pid, "changed": changed, "file": subset})

        time.sleep(FILE_EVERY)
    emit({"kind": "sampler_stop"})


if __name__ == "__main__":
    main()
