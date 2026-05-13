#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

DATA_DIR="${VOIDNET_BOOTSTRAP_DATA_DIR:-$HOME/.voidnet/bootstrap}"
LISTEN_ADDR="${VOIDNET_BOOTSTRAP_LISTEN:-/ip4/0.0.0.0/udp/7000/quic-v1}"
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
PUBLIC_IP="<PUBLIC_IP>"
if command -v curl >/dev/null 2>&1; then
  if RESOLVED_IP="$(curl -fsS https://api.ipify.org 2>/dev/null)"; then
    if [[ -n "$RESOLVED_IP" ]]; then
      PUBLIC_IP="$RESOLVED_IP"
    fi
  fi
fi

PUBLIC_MULTIADDR="/ip4/$PUBLIC_IP/udp/7000/quic-v1/p2p/$PEER_ID"

cat <<EOF
[VOIDNET][WAN] Bootstrap relay node
data_dir=$DATA_DIR
listen_addr=$LISTEN_ADDR
peer_id=$PEER_ID
public_multiaddr=$PUBLIC_MULTIADDR

[VOIDNET][WAN] Client bootstrap.toml
bootstrap_nodes = [
  "$PUBLIC_MULTIADDR"
]

[VOIDNET][WAN] Operator checks
cargo run -p void-cli -- --data-dir "$DATA_DIR" network bootstrap
cargo run -p void-cli -- --data-dir "$DATA_DIR" network reachability
cargo run -p void-cli -- --data-dir "$DATA_DIR" network relays
cargo run -p void-cli -- --data-dir "$DATA_DIR" network sessions
cargo run -p void-cli -- --data-dir "$DATA_DIR" network diagnostics

[VOIDNET][WAN] Firewall
sudo ufw allow 7000/udp
sudo ufw status
EOF

exec "${NODE_CMD[@]}" \
  --data-dir "$DATA_DIR" \
  --listen "$LISTEN_ADDR" \
  --relay-server