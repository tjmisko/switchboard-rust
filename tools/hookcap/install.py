#!/usr/bin/env python3
"""Install, remove, or check the signal-capture experiment.

  install.py install     copy scripts, back up and patch both configs, start the sampler
  install.py uninstall   remove every hookcap entry, restore the status line, stop the sampler
  install.py status      show what is installed and how much has been recorded today

The existing switchboard-ctl hooks are never touched: hookcap entries are added
as separate hook groups beside them, and removed by matching the install path.
Recordings in ~/.local/state/switchboard/hookcap are kept on uninstall.
"""

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

HOME = Path.home()
SRC = Path(__file__).resolve().parent
BIN = HOME / ".local/share/switchboard-hookcap"
DATA = HOME / ".local/state/switchboard/hookcap"
CLAUDE = HOME / ".claude/settings.json"
CODEX = HOME / ".codex/hooks.json"
UNIT = "switchboard-hookcap-sampler"
SCRIPTS = ["hookcap", "statusline-tee", "subagent-statusline", "sampler.py", "analyze.py"]

# Verified against the 2.1.270 bundle's hook event list. Never add a name that
# is not in it: an unknown event can invalidate the whole settings file.
CLAUDE_EVENTS = [
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PermissionDenied",
    "PostToolUse", "PostToolUseFailure", "Stop", "StopFailure", "Notification", "Elicitation",
    "ElicitationResult", "SubagentStart", "SubagentStop", "TeammateIdle", "TaskCreated",
    "TaskCompleted", "CwdChanged", "PreCompact", "PostCompact", "SessionEnd",
]
# Verified against the Codex 0.154 binary (docs/product/survey/14 §2).
CODEX_EVENTS = [
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse", "Stop",
    "SubagentStart", "SubagentStop", "Interrupt", "PreCompact", "PostCompact", "SessionEnd",
]


def load(path):
    return json.loads(path.read_text())


def save(path, data):
    """Back up once per run, then replace atomically through any symlink."""
    real = path.resolve()
    stamp = time.strftime("%Y%m%d-%H%M%S")
    backup = real.with_name(f"{real.name}.hookcap-backup-{stamp}")
    n = 1
    while backup.exists():  # never overwrite an earlier, possibly pristine, backup
        n += 1
        backup = real.with_name(f"{real.name}.hookcap-backup-{stamp}-{n}")
    shutil.copy2(real, backup)
    tmp = real.with_name(f".{real.name}.hookcap-tmp")
    tmp.write_text(json.dumps(data, indent=2) + "\n")
    os.replace(tmp, real)
    print(f"  wrote {path} (backup {backup.name})")


def ours(group):
    return any(str(BIN) in h.get("command", "") for h in group.get("hooks", []))


def add_hooks(config, provider, events):
    hooks = config.setdefault("hooks", {})
    added = 0
    for event in events:
        groups = hooks.setdefault(event, [])
        if any(ours(g) for g in groups):
            continue
        groups.append({"hooks": [{"type": "command", "command": f"{BIN}/hookcap {provider} {event}", "timeout": 2}]})
        added += 1
    return added


def remove_hooks(config):
    hooks = config.get("hooks", {})
    removed = 0
    for event in list(hooks):
        kept = [g for g in hooks[event] if not ours(g)]
        removed += len(hooks[event]) - len(kept)
        if kept:
            hooks[event] = kept
        else:
            del hooks[event]
    return removed


def install():
    BIN.mkdir(parents=True, exist_ok=True)
    for name in SCRIPTS:
        shutil.copy2(SRC / name, BIN / name)
        (BIN / name).chmod(0o755)
    print(f"  copied scripts to {BIN}")

    claude = load(CLAUDE)
    n = add_hooks(claude, "claude", CLAUDE_EVENTS)
    status = claude.get("statusLine")
    if status and status.get("type") == "command" and str(BIN) not in status["command"]:
        status["command"] = f"{BIN}/statusline-tee {status['command']}"
    if not claude.get("subagentStatusLine"):
        claude["subagentStatusLine"] = {"type": "command", "command": f"{BIN}/subagent-statusline"}
    elif str(BIN) not in claude["subagentStatusLine"].get("command", ""):
        print("  subagentStatusLine is already set by you; leaving it alone (panel cadence will not be recorded)")
    save(CLAUDE, claude)
    print(f"  claude: {n} hook groups added, status line wrapped, subagent status line recorded")

    codex = load(CODEX)
    n = add_hooks(codex, "codex", CODEX_EVENTS)
    save(CODEX, codex)
    print(f"  codex: {n} hook groups added")

    subprocess.run(["systemctl", "--user", "stop", UNIT], capture_output=True)
    subprocess.run(["systemctl", "--user", "reset-failed", UNIT], capture_output=True)
    subprocess.run(["systemd-run", "--user", f"--unit={UNIT}", "--collect",
                    "python3", str(BIN / "sampler.py")], check=True, capture_output=True)
    print(f"  sampler running as {UNIT}.service")
    print("\nNew Claude and Codex sessions pick this up. Restart long-running ones, or check /hooks.")


def uninstall():
    subprocess.run(["systemctl", "--user", "stop", UNIT], capture_output=True)
    print("  sampler stopped")

    claude = load(CLAUDE)
    n = remove_hooks(claude)
    status = claude.get("statusLine")
    prefix = f"{BIN}/statusline-tee "
    if status and status.get("command", "").startswith(prefix):
        status["command"] = status["command"][len(prefix):]
    if str(BIN) in (claude.get("subagentStatusLine") or {}).get("command", ""):
        del claude["subagentStatusLine"]
    save(CLAUDE, claude)
    print(f"  claude: {n} hook groups removed, status line restored")

    codex = load(CODEX)
    n = remove_hooks(codex)
    save(CODEX, codex)
    print(f"  codex: {n} hook groups removed")
    print(f"\nRecordings kept in {DATA}")


def status():
    claude, codex = load(CLAUDE), load(CODEX)
    count = lambda cfg: sum(ours(g) for gs in cfg.get("hooks", {}).values() for g in gs)
    print(f"claude hook groups: {count(claude)}/{len(CLAUDE_EVENTS)}")
    print(f"codex hook groups:  {count(codex)}/{len(CODEX_EVENTS)}")
    print(f"status line wrapped: {str(BIN) in (claude.get('statusLine') or {}).get('command', '')}")
    result = subprocess.run(["systemctl", "--user", "is-active", UNIT], capture_output=True, text=True)
    active = result.stdout.strip()
    print(f"sampler: {active or 'unknown (systemctl query failed: ' + result.stderr.strip() + ')'}")
    today = DATA / time.strftime("%Y-%m-%d")
    for name in ["hooks.jsonl", "statusline.jsonl", "subagents.jsonl", "sampler.jsonl"]:
        path = today / name
        lines = sum(1 for _ in path.open()) if path.exists() else 0
        size = path.stat().st_size if path.exists() else 0
        print(f"  {name:18} {lines:7} records  {size / 1024:8.1f} KiB")


if __name__ == "__main__":
    commands = {"install": install, "uninstall": uninstall, "status": status}
    if len(sys.argv) != 2 or sys.argv[1] not in commands:
        sys.exit(__doc__)
    commands[sys.argv[1]]()
