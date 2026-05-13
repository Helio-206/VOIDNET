#!/usr/bin/env bash
set -euo pipefail

LAB_DIR="/tmp/voidnet-labs/swarm-simulator"
mkdir -p "$LAB_DIR/a" "$LAB_DIR/b" "$LAB_DIR/c"

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/a" \
  --listen /ip4/127.0.0.1/udp/39201/quic-v1 \
  --exit-after-secs 90 &
NODE_A=$!

sleep 3

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/b" \
  --listen /ip4/127.0.0.1/udp/39202/quic-v1 \
  --bootstrap /ip4/127.0.0.1/udp/39201/quic-v1 \
  --exit-after-secs 90 &
NODE_B=$!

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/c" \
  --listen /ip4/127.0.0.1/udp/39203/quic-v1 \
  --bootstrap /ip4/127.0.0.1/udp/39201/quic-v1 \
  --exit-after-secs 90 &
NODE_C=$!

wait "$NODE_A" "$NODE_B" "$NODE_C"

