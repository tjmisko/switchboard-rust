#!/usr/bin/env python3
"""Adversarial audit: fd exhaustion -> accept spin -> backlog full -> the hook
forwarder blocks in connect() and stalls the agent's hook."""
import os, resource, signal, socket, subprocess, sys, time
sys.path.insert(0, os.path.dirname(__file__))
import adv_daemon_fuzz as f

p = f.start(nofile=64)
held = []
try:
    time.sleep(1.5)
    for i in range(int(sys.argv[1]) if len(sys.argv) > 1 else 5000):
        c = socket.socket(socket.AF_UNIX)
        c.setblocking(False)
        try:
            c.connect(f.SOCK)
        except BlockingIOError:
            eagain = globals().get("eagain", 0) + 1
            globals()["eagain"] = eagain
            c.close()
            continue
        except OSError as e:
            print("   client", i, "connect:", e)
            c.close()
            break
        held.append(c)
    print("   held client sockets:", len(held), "EAGAIN (backlog full):", globals().get("eagain", 0))
    t0 = f.stat(p.pid)["cpu_ticks"]; time.sleep(2); t1 = f.stat(p.pid)["cpu_ticks"]
    print("   daemon CPU while held:", (t1 - t0) / os.sysconf("SC_CLK_TCK") / 2 * 100, "%")
    time.sleep(1)
    print("   ss:", subprocess.run(["ss", "-xlp"], capture_output=True, text=True).stdout.splitlines()[0])
    print("   ss:", [l for l in subprocess.run(["ss", "-xl"], capture_output=True, text=True).stdout.splitlines() if "sbr-audit" in l])
    topped = 0
    while True:
        c = socket.socket(socket.AF_UNIX); c.setblocking(False)
        try:
            c.connect(f.SOCK); held.append(c); topped += 1
        except BlockingIOError:
            c.close(); break
    print("   topped up", topped, "more until EAGAIN")
    env = f.env(); env["SWITCHBOARD_DEBUG"] = "1"
    t = time.monotonic()
    r = subprocess.run(["timeout", "10", f.HOOK, "Stop"], input=b'{"session_id":"s"}', env=env)
    print(f"   forwarder rc={r.returncode} after {time.monotonic() - t:.2f}s (124 = killed by timeout 10s)")
finally:
    for c in held:
        c.close()
    time.sleep(0.5)
    print("   daemon SIGTERM exit", f.stop(p), "rc", p.returncode)
