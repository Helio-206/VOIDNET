#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 3 ]]; then
  cat <<'EOF'
usage: scripts/wan/connect-client.sh <bootstrap_multiaddr> [data_dir] [listen_port]

example:
scripts/wan/connect-client.sh "/ip4/203.0.113.10/udp/7000/quic-v1/p2p/12D3Koo..." "$HOME/.voidnet/machine-a" 0
EOF
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

BOOTSTRAP_ADDR="$1"
DATA_DIR="${2:-$HOME/.voidnet/client}"
LISTEN_PORT="${3:-0}"
LISTEN_ADDR="/ip4/0.0.0.0/udp/$LISTEN_PORT/quic-v1"
mkdir -p "$DATA_DIR"

if [[ -x "$REPO_ROOT/target/debug/void" ]]; then
  CLI_CMD=("$REPO_ROOT/target/debug/void")
else
  CLI_CMD=(cargo run -q -p void-cli --)
fi

if [[ -x "$REPO_ROOT/target/debug/void-node" ]]; then
  NODE_CMD=("$REPO_ROOT/target/debug/void-node")
else
  NODE_CMD=(cargo run -q -p void-node --)
fi

PEER_ID="$(${CLI_CMD[@]} --data-dir "$DATA_DIR" identity --persistent | sed -n 's/^peer_id=//p' | head -n 1)"

cat > "$DATA_DIR/bootstrap.toml" <<EOF
bootstrap_nodes = [
  "$BOOTSTRAP_ADDR"
]
EOF

cat <<EOF
[VOIDNET][WAN] Client node
data_dir=$DATA_DIR
listen_addr=$LISTEN_ADDR
peer_id=$PEER_ID
bootstrap=$BOOTSTRAP_ADDR
bootstrap_file=$DATA_DIR/bootstrap.toml

[VOIDNET][WAN] Diagnostics
cargo run -p void-cli -- --data-dir "$DATA_DIR" network bootstrap
cargo run -p void-cli -- --data-dir "$DATA_DIR" network reachability
cargo run -p void-cli -- --data-dir "$DATA_DIR" network relays
cargo run -p void-cli -- --data-dir "$DATA_DIR" network sessions
cargo run -p void-cli -- --data-dir "$DATA_DIR" network peers
cargo run -p void-cli -- --data-dir "$DATA_DIR" network diagnostics

[VOIDNET][WAN] Console
cargo run -p void-cli -- --data-dir "$DATA_DIR" console
EOF

exec "${NODE_CMD[@]}" \
  --data-dir "$DATA_DIR" \
  --listen "$LISTEN_ADDR" \
  --bootstrap "$BOOTSTRAP_ADDR" \
  --bootstrap-config "$DATA_DIR/bootstrap.toml" \
  --no-mdns