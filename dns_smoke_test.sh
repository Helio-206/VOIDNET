#!/bin/bash
source $HOME/.cargo/env

rm -rf /tmp/voidnet-dns-a /tmp/voidnet-dns-b
mkdir -p /tmp/voidnet-dns-a /tmp/voidnet-dns-b

# Start node A
cargo run -p void-node -- --data-dir /tmp/voidnet-dns-a --listen /ip4/127.0.0.1/udp/39501/quic-v1 --exit-after-secs 20 > /tmp/voidnet-dns-a.log 2>&1 &
PID_A=$!

# Wait for A to be ready
sleep 5

# Start node B bootstrapping to A
NODE_A_PEERID=$(grep "Local PeerId" /tmp/voidnet-dns-a.log | awk '{print $NF}')
if [ -z "$NODE_A_PEERID" ]; then
  # Fallback: check log more thoroughly
  NODE_A_PEERID=$(grep -o "12D3Koo[a-zA-Z0-9]*" /tmp/voidnet-dns-a.log | head -n 1)
fi

echo "Node A PeerID: $NODE_A_PEERID"

cargo run -p void-node -- --data-dir /tmp/voidnet-dns-b --listen /ip4/127.0.0.1/udp/39502/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39501/quic-v1/p2p/$NODE_A_PEERID --exit-after-secs 20 > /tmp/voidnet-dns-b.log 2>&1 &
PID_B=$!

sleep 5

# Queue DNS publish on A
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-a dns publish chat.void

# Wait for nodes to exit or timeout
wait $PID_A $PID_B

echo "--- Node A Resolve ---"
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-a dns resolve chat.void
echo "--- Node B Resolve ---"
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-b dns resolve chat.void
echo "--- Node B Cache ---"
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-b dns cache
