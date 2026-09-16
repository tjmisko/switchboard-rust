#!/usr/bin/env python3
"""Adversarial audit: fuzz the real `switchboard` daemon over its socket.

Usage: python3 tests/adv_daemon_fuzz.py [phase ...]
Phases: malformed huge concurrent slowloris subscribers emfile sigterm
The daemon runs against a private socket and private XDG dirs under
~/.cache/sbr-audit/state. It is always killed on exit.
"""
import json
import os
import random
import resource
import signal
import socket
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target/release/switchboard")
HOOK = os.path.join(ROOT, "target/release/switchboard-hook")
SOCK = os.path.join(os.environ["XDG_RUNTIME_DIR"], "sbr-audit.sock")
STATE = os.path.expanduser("~/.cache/sbr-audit/state")

soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
resource.setrlimit(resource.RLIMIT_NOFILE, (min(hard, 65536), hard))


def env():
    e = dict(os.environ)
    e["SWITCHBOARD_RUST_SOCKET"] = SOCK
    e["XDG_CACHE_HOME"] = os.path.join(STATE, "cache")
    e["XDG_STATE_HOME"] = os.path.join(STATE, "state")
    return e


def start(nofile=None):
    os.makedirs(STATE, exist_ok=True)
    cmd = [BIN]
    if nofile:
        cmd = ["bash", "-c", f"ulimit -n {nofile}; exec {BIN}"]
    p = subprocess.Popen(cmd, env=env(), stderr=open(os.path.join(STATE, "daemon.err"), "ab"))
    for _ in range(100):
        if os.path.exists(SOCK):
            try:
                s = socket.socket(socket.AF_UNIX)
                s.connect(SOCK)
                s.close()
                return p
            except OSError:
                pass
        time.sleep(0.05)
    raise SystemExit("daemon did not come up")


def stat(pid):
    with open(f"/proc/{pid}/status") as f:
        st = dict(l.split(":", 1) for l in f if ":" in l)
    with open(f"/proc/{pid}/stat") as f:
        fields = f.read().rsplit(")", 1)[1].split()
    ticks = int(fields[11]) + int(fields[12])
    fds = len(os.listdir(f"/proc/{pid}/fd"))
    return {
        "rss_kb": int(st["VmRSS"].split()[0]),
        "threads": int(st["Threads"]),
        "fds": fds,
        "cpu_ticks": ticks,
    }


def request(line, timeout=2.0):
    t = time.monotonic()
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(timeout)
    s.connect(SOCK)
    s.sendall(line if isinstance(line, bytes) else line.encode())
    buf = b""
    try:
        while not buf.endswith(b"\n"):
            chunk = s.recv(1 << 16)
            if not chunk:
                break
            buf += chunk
    except socket.timeout:
        buf += b"<TIMEOUT>"
    s.close()
    return buf, time.monotonic() - t


def list_latency():
    try:
        reply, dt = request(b'{"cmd":"list"}\n', timeout=3.0)
        ok = reply.startswith(b'{"snapshot"')
        return ok, dt
    except OSError as e:
        return False, repr(e)


def phase_malformed(p):
    cases = [
        b"\n",
        b"not json\n",
        b"[]\n",
        b"null\n",
        b'"str"\n',
        b"{}\n",
        b'{"cmd":1}\n',
        b'{"cmd":"hook"}\n',
        b'{"cmd":"hook","event":"Stop"}\n',
        b'{"cmd":"hook","event":"Stop","session_id":"s","pid":-1}\n',
        b'{"cmd":"hook","event":"Stop","session_id":"s","pid":1.5}\n',
        b'{"cmd":"hook","event":"Stop","session_id":"s","pid":99999999999}\n',
        b'{"cmd":"hook","event":"Stop","session_id":"s","pid":1e400}\n',
        b'{"cmd":"statusline","session_id":"s","rate_limits":{"status":"rejected","resetsAt":18446744073709551615,"utilization":1e308}}\n',
        b'{"cmd":"statusline","session_id":"s","cost":{"total_api_duration_ms":18446744073709551615},"context_window":{"total_input_tokens":99999999999999}}\n',
        b'{"cmd":"hook","event":"PreToolUse","session_id":"s","pid":1,"tool_use_id":"x","tool_name":"AskUserQuestion","tool_input_hash":"zzzzzzzzzzzzzzzz"}\n',
        b'{"cmd":"hook","event":"Stop","session_id":"s","background_tasks":"notarray","session_crons":[1,2,{"id":5}]}\n',
        b"\xff\xfe\xfd garbage \x00\n",
        b'{"cmd":"list"' + b"\x00" * 10 + b"}\n",
        b"[" * 100000 + b"\n",
        b'{"a":' * 5000 + b"1" + b"}" * 5000 + b"\n",
        b'{"cmd":"hook","event":"SessionStart","session_id":"' + "☃".encode() * 1000 + b'","pid":1}\n',
    ]
    results = {}
    for c in cases:
        reply, dt = request(c)
        key = c[:50]
        results[key] = (reply[:90], round(dt, 3))
    for k, v in results.items():
        print("  ", k, "->", v)
    # Random bytes
    rnd = random.Random(7)
    no_reply = 0
    for _ in range(500):
        n = rnd.randint(0, 400)
        data = bytes(rnd.getrandbits(8) for _ in range(n)) + b"\n"
        reply, _ = request(data)
        if not reply:
            no_reply += 1
    print(f"   random 500 lines: {no_reply} got no reply (connection closed silently)")
    print("   alive:", p.poll() is None, "list:", list_latency())


def phase_huge(p):
    # 2 MiB with no newline, then a valid request on the same connection.
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(5)
    s.connect(SOCK)
    try:
        s.sendall(b"x" * (2 << 20))
    except OSError as e:
        print("   send huge:", e)
    try:
        s.sendall(b'\n{"cmd":"list"}\n')
    except OSError as e:
        print("   send after huge:", e)
    buf = b""
    try:
        while True:
            c = s.recv(1 << 16)
            if not c:
                break
            buf += c
    except socket.timeout:
        buf += b"<TIMEOUT>"
    except OSError as e:
        buf += repr(e).encode()
    s.close()
    print("   2MiB unterminated then list ->", buf[:120])
    # 900 KiB valid JSON with a newline.
    big = json.dumps({"cmd": "hook", "event": "Stop", "session_id": "s", "pad": "y" * (900 << 10)}) + "\n"
    print("   900KiB valid frame ->", request(big.encode(), timeout=5)[0][:80])
    print("   alive:", p.poll() is None, "list:", list_latency(), stat(p.pid))


def phase_concurrent(p):
    before = stat(p.pid)
    socks = []
    t = time.monotonic()
    for i in range(3000):
        s = socket.socket(socket.AF_UNIX)
        s.setblocking(True)
        s.connect(SOCK)
        s.sendall(b'{"cmd":"hook","event":"Stop","session_id":"none","pid":1}\n')
        socks.append(s)
    replies = 0
    for s in socks:
        s.settimeout(5)
        try:
            if s.recv(64).startswith(b'{"ok":true}'):
                replies += 1
        except OSError:
            pass
        s.close()
    print(f"   3000 concurrent hook conns: {replies} ok in {time.monotonic() - t:.2f}s")
    time.sleep(1)
    print("   before", before, "after", stat(p.pid), "list", list_latency())


def phase_slowloris(p, n=5000):
    before = stat(p.pid)
    socks = []
    for i in range(n):
        s = socket.socket(socket.AF_UNIX)
        try:
            s.connect(SOCK)
        except OSError as e:
            print("   connect failed at", i, e)
            break
        s.send(b'{"cmd":"hook"')  # never finished
        socks.append(s)
    time.sleep(2)
    mid = stat(p.pid)
    print(f"   {len(socks)} idle half-line connections held: before {before} during {mid} list {list_latency()}")
    for s in socks:
        s.close()
    time.sleep(2)
    print("   after close:", stat(p.pid))


def phase_subscribers(p):
    """Subscribers that never read, while the view churns."""
    before = stat(p.pid)
    subs = []
    for i in range(300):
        s = socket.socket(socket.AF_UNIX)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4096)
        s.connect(SOCK)
        s.sendall(b'{"cmd":"subscribe"}\n')
        subs.append(s)
    # Churn the view: a statusline beat does not publish, but CwdChanged on a
    # tracked agent changes `cwd` in the view. Use the real agents' pids.
    agents = json.loads(request(b'{"cmd":"list"}\n')[0])["snapshot"]["agents"]
    print("   tracked agents:", [(a["pid"], a["phase"]) for a in agents])
    if not agents:
        print("   no agents to churn; skipping")
        return
    a = agents[0]
    t_end = time.monotonic() + 20
    n = 0
    lat = []
    while time.monotonic() < t_end:
        cwd = f"/audit/{n % 2}"
        frame = {"cmd": "hook", "event": "CwdChanged", "pid": a["pid"], "session_id": a["session_id"], "new_cwd": cwd}
        request(json.dumps(frame).encode() + b"\n")
        n += 1
        if n % 200 == 0:
            lat.append(list_latency()[1])
    during = stat(p.pid)
    print(f"   300 non-reading subscribers, {n} view changes in 20s; list latencies {[round(x, 4) if isinstance(x, float) else x for x in lat[:8]]}")
    print("   before", before, "during", during)
    # A reading subscriber still gets updates?
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(3)
    s.connect(SOCK)
    s.sendall(b'{"cmd":"subscribe"}\n')
    first = s.recv(1 << 20)
    request(json.dumps({"cmd": "hook", "event": "CwdChanged", "pid": a["pid"], "session_id": a["session_id"], "new_cwd": "/audit/final"}).encode() + b"\n")
    try:
        second = s.recv(1 << 20)
    except socket.timeout:
        second = b"<TIMEOUT>"
    print("   reading subscriber got update:", b"/audit/final" in second)
    for s2 in subs:
        s2.close()
    s.close()
    # Put the cwd back so the view is sane.
    time.sleep(1)
    print("   after closing subscribers:", stat(p.pid))


def phase_emfile(_p):
    """Daemon with a small fd limit: exhaust it with idle connections and
    measure CPU while they are held."""
    p = start(nofile=64)
    try:
        time.sleep(1.5)
        base = stat(p.pid)
        socks = []
        for i in range(200):
            s = socket.socket(socket.AF_UNIX)
            s.settimeout(1)
            try:
                s.connect(SOCK)
                socks.append(s)
            except OSError as e:
                print("   client connect failed at", i, e)
                break
        t0 = stat(p.pid)
        time.sleep(5)
        t1 = stat(p.pid)
        hz = os.sysconf("SC_CLK_TCK")
        cpu = (t1["cpu_ticks"] - t0["cpu_ticks"]) / hz / 5
        print(f"   nofile=64, {len(socks)} idle client conns: daemon CPU {cpu * 100:.0f}% of a core over 5s; base {base} held {t1}")
        ok, dt = list_latency()
        print("   list while exhausted:", ok, dt)
        for s in socks:
            s.close()
        time.sleep(1)
        t2 = stat(p.pid)
        time.sleep(3)
        t3 = stat(p.pid)
        print(f"   after release: CPU {(t3['cpu_ticks'] - t2['cpu_ticks']) / hz / 3 * 100:.0f}%; list {list_latency()}")
    finally:
        stop(p)


def stop(p):
    if p.poll() is None:
        t = time.monotonic()
        p.send_signal(signal.SIGTERM)
        try:
            p.wait(10)
        except subprocess.TimeoutExpired:
            print("   daemon did not exit within 10s of SIGTERM; killing")
            p.kill()
            p.wait()
        return time.monotonic() - t
    return None


def phase_sigterm(_p):
    p = start()
    # Leave a blocked subscriber and a half-line connection open.
    blocked = socket.socket(socket.AF_UNIX)
    blocked.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
    blocked.connect(SOCK)
    blocked.sendall(b'{"cmd":"subscribe"}\n')
    half = socket.socket(socket.AF_UNIX)
    half.connect(SOCK)
    half.send(b'{"cmd":')
    time.sleep(1)
    dt = stop(p)
    print(f"   SIGTERM exit in {dt:.3f}s rc={p.returncode} socket removed={not os.path.exists(SOCK)}")
    blocked.close()
    half.close()


PHASES = {
    "malformed": phase_malformed,
    "huge": phase_huge,
    "concurrent": phase_concurrent,
    "slowloris": phase_slowloris,
    "subscribers": phase_subscribers,
    "emfile": phase_emfile,
    "sigterm": phase_sigterm,
}


def main():
    wanted = sys.argv[1:] or list(PHASES)
    shared = [w for w in wanted if w not in ("emfile", "sigterm")]
    p = None
    try:
        if shared:
            p = start()
            print("daemon pid", p.pid, "start", stat(p.pid))
        for w in wanted:
            print(f"== {w}")
            PHASES[w](p)
        if p:
            print("final", stat(p.pid))
            dt = stop(p)
            print(f"SIGTERM exit {dt:.3f}s rc={p.returncode} socket removed={not os.path.exists(SOCK)}")
    finally:
        if p and p.poll() is None:
            p.kill()


if __name__ == "__main__":
    main()
