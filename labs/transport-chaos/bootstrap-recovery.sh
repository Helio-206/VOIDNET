#!/usr/bin/env bash
set -euo pipefail

LAB_DIR="/tmp/voidnet-labs/bootstrap-recovery"
mkdir -p "$LAB_DIR/bootstrap" "$LAB_DIR/peer"

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/bootstrap" \
  --listen /ip4/127.0.0.1/udp/39401/quic-v1 \
  --exit-after-secs 20 || true

sleep 3

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/bootstrap" \
  --listen /ip4/127.0.0.1/udp/39401/quic-v1 \
  --exit-after-secs 90 &
BOOTSTRAP=$!

sleep 3

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/peer" \
  --listen /ip4/127.0.0.1/udp/39402/quic-v1 \
  --bootstrap /ip4/127.0.0.1/udp/39401/quic-v1 \
  --exit-after-secs 60

wait "$BOOTSTRAP"

