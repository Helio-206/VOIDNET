#!/usr/bin/env bash
set -euo pipefail

LAB_DIR="/tmp/voidnet-labs/reconnect"
mkdir -p "$LAB_DIR/a" "$LAB_DIR/b"

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/a" \
  --listen /ip4/127.0.0.1/udp/39301/quic-v1 \
  --exit-after-secs 120 &
NODE_A=$!

sleep 3

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/b" \
  --listen /ip4/127.0.0.1/udp/39302/quic-v1 \
  --bootstrap /ip4/127.0.0.1/udp/39301/quic-v1 \
  --exit-after-secs 25 || true

sleep 5

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/b" \
  --listen /ip4/127.0.0.1/udp/39302/quic-v1 \
  --bootstrap /ip4/127.0.0.1/udp/39301/quic-v1 \
  --exit-after-secs 45 || true

wait "$NODE_A"

