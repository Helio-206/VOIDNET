#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAB_DIR="/tmp/voidnet-labs/swarm-simulator"
mkdir -p "$LAB_DIR/a" "$LAB_DIR/b"

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/a" \
  --listen /ip4/127.0.0.1/udp/39101/quic-v1 \
  --exit-after-secs 60 &
NODE_A=$!

sleep 3

cargo run -p void-node -- \
  --data-dir "$LAB_DIR/b" \
  --listen /ip4/127.0.0.1/udp/39102/quic-v1 \
  --bootstrap /ip4/127.0.0.1/udp/39101/quic-v1 \
  --exit-after-secs 60 &
NODE_B=$!

wait "$NODE_A" "$NODE_B"

