#!/usr/bin/env bash
# Adversarial audit: the switchboard-hook forwarder must exit 0, quickly,
# whatever happens. Run from the crate root after `cargo build --release`.
set -u
HOOK="$PWD/target/release/switchboard-hook"
export SWITCHBOARD_RUST_SOCKET="$XDG_RUNTIME_DIR/sbr-audit-fwd.sock"
SCRATCH="$HOME/.cache/sbr-audit/fwd"
mkdir -p "$SCRATCH"
rm -f "$SWITCHBOARD_RUST_SOCKET"

run() {
    local label="$1"; shift
    local start end rc
    start=$(date +%s%N)
    "$@"
    rc=$?
    end=$(date +%s%N)
    printf '  %-58s rc=%s %6d ms\n' "$label" "$rc" $(( (end - start) / 1000000 ))
}

echo "== daemon down (no socket file)"
run "garbage stdin" bash -c "head -c 5000 /dev/urandom | '$HOOK' PreToolUse"
run "empty stdin" bash -c "'$HOOK' Stop < /dev/null"
run "valid payload" bash -c "echo '{\"session_id\":\"s\",\"tool_input\":{\"a\":1}}' | '$HOOK' PermissionRequest"
run "no argv, statusline" bash -c "echo '[]' | '$HOOK' statusline"
run "8 MiB stdin" bash -c "head -c 8000000 /dev/zero | tr '\\0' 'a' | '$HOOK' Stop"

echo "== socket path is a regular file"
echo x >| "$SWITCHBOARD_RUST_SOCKET"
run "valid payload" bash -c "echo '{\"session_id\":\"s\"}' | '$HOOK' Stop"
rm -f "$SWITCHBOARD_RUST_SOCKET"

echo "== server accepts and never replies"
python3 - "$SWITCHBOARD_RUST_SOCKET" <<'EOF' &
import socket, sys, time
s = socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(64)
conns = []
end = time.time() + 20
s.settimeout(0.5)
while time.time() < end:
    try:
        c, _ = s.accept(); conns.append(c)
    except socket.timeout:
        pass
EOF
SERVER=$!
sleep 0.5
run "valid payload" bash -c "echo '{\"session_id\":\"s\"}' | '$HOOK' Stop"
kill $SERVER 2>/dev/null; wait $SERVER 2>/dev/null
rm -f "$SWITCHBOARD_RUST_SOCKET"

echo "== server listening but never accepting (stopped daemon), backlog full"
python3 - "$SWITCHBOARD_RUST_SOCKET" <<'EOF' &
import socket, sys, time
s = socket.socket(socket.AF_UNIX); s.bind(sys.argv[1]); s.listen(1)
time.sleep(30)
EOF
SERVER=$!
sleep 0.5
# Fill the backlog with clients that connect and hold.
python3 - "$SWITCHBOARD_RUST_SOCKET" <<'EOF' &
import socket, sys, time
held = []
for i in range(4):
    c = socket.socket(socket.AF_UNIX); c.setblocking(False)
    try:
        c.connect(sys.argv[1]); held.append(c)
    except BlockingIOError:
        held.append(c)
    except OSError as e:
        print("   filler", i, e)
time.sleep(30)
EOF
FILLER=$!
sleep 0.5
run "valid payload (timeout 5s)" timeout 5 bash -c "echo '{\"session_id\":\"s\"}' | '$HOOK' Stop"
kill $SERVER $FILLER 2>/dev/null; wait $SERVER $FILLER 2>/dev/null
rm -f "$SWITCHBOARD_RUST_SOCKET"
