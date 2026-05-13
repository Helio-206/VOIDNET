# VOIDNET Real Mesh Smoke

This procedure validates real multi-machine connectivity over the internet, not localhost-only behavior.

## Scope

Success criteria for this smoke:

1. A stable bootstrap node is reachable from two different machines.
2. Both machines discover the same mesh over WAN.
3. Encrypted peer sessions establish and stay visible in diagnostics.
4. The same room synchronizes on both machines.
5. Messages move live across different networks.
6. Temporary disconnects recover without losing runtime state.

## Topology

- Bootstrap node: public VPS with UDP/QUIC exposed.
- Machine A: home or office network.
- Machine B: different network, ideally hotspot or another ISP.

Recommended bootstrap listen address:

```toml
bootstrap_nodes = [
  "/ip4/<BOOTSTRAP_PUBLIC_IP>/udp/7000/quic-v1/p2p/<BOOTSTRAP_PEER_ID>"
]
```

## Bootstrap Setup

On the VPS:

1. Ensure UDP `7000` is open in the VPS firewall and cloud security group.
2. Recommended path: use the helper script:

```sh
scripts/wan/start-bootstrap-relay.sh
```

3. Manual path, if you need full control: start the node once to persist identity:

```sh
cargo run -p void-node -- \
  --data-dir /var/lib/voidnet/bootstrap \
  --listen /ip4/0.0.0.0/udp/7000/quic-v1 \
  --relay-server
```

4. Record the persistent peer id:

```sh
cargo run -p void-cli -- --data-dir /var/lib/voidnet/bootstrap identity --persistent
```

5. Create `/var/lib/voidnet/bootstrap/bootstrap.toml`:

```toml
bootstrap_nodes = []
```

6. Restart the bootstrap node and keep it running:

```sh
cargo run -p void-node -- \
  --data-dir /var/lib/voidnet/bootstrap \
  --listen /ip4/0.0.0.0/udp/7000/quic-v1 \
  --relay-server \
  --bootstrap-config /var/lib/voidnet/bootstrap/bootstrap.toml
```

Expected result:

- Node identity stays stable across restarts.
- `topology.json` and `events.log` persist in the same data directory.
- The VPS peer id becomes the stable bootstrap id used by other machines.

## Machine Setup

On Machine A and Machine B, create a bootstrap file in the node data directory:

```toml
bootstrap_nodes = [
  "/ip4/<BOOTSTRAP_PUBLIC_IP>/udp/7000/quic-v1/p2p/<BOOTSTRAP_PEER_ID>"
]
```

Start each node with a dedicated data directory:

```sh
cargo run -p void-node -- \
  --data-dir ~/.voidnet-a \
  --listen /ip4/0.0.0.0/udp/0/quic-v1 \
  --bootstrap-config ~/.voidnet-a/bootstrap.toml
```

```sh
cargo run -p void-node -- \
  --data-dir ~/.voidnet-b \
  --listen /ip4/0.0.0.0/udp/0/quic-v1 \
  --bootstrap-config ~/.voidnet-b/bootstrap.toml
```

## Operator Checks

On each machine, verify live network state:

```sh
scripts/wan/smoke-check.sh ~/.voidnet-a
```

Expected output signals:

- `reachability=PUBLIC` or `reachability=PRIVATE` with at least one observed WAN address after identify exchange.
- `bootstrap_connected=1` or more.
- `relay_reservations=1` or more if the client obtained a relay reservation.
- At least one peer with `PATH DIRECT` when direct WAN connectivity succeeds.
- `encrypted_sessions` greater than zero in diagnostics.

## Chat Validation

On Machine A:

```sh
cargo run -p void-cli -- --data-dir ~/.voidnet-a chat join operators
cargo run -p void-cli -- --data-dir ~/.voidnet-a chat switch operators
cargo run -p void-cli -- --data-dir ~/.voidnet-a chat room-send operators "hello from machine a"
```

On Machine B:

```sh
cargo run -p void-cli -- --data-dir ~/.voidnet-b chat join operators
cargo run -p void-cli -- --data-dir ~/.voidnet-b chat switch operators
cargo run -p void-cli -- --data-dir ~/.voidnet-b chat inbox
```

Expected result:

- Room `operators` exists on both machines.
- Machine B receives the encrypted message from Machine A.
- Presence and room membership events appear in `events.log`.

Reply from Machine B:

```sh
cargo run -p void-cli -- --data-dir ~/.voidnet-b chat room-send operators "reply from machine b"
```

Then verify Machine A inbox and room state.

## Reconnect Validation

1. Disconnect Machine B from the network for 30 to 60 seconds.
2. Reconnect Machine B.
3. Check:

```sh
cargo run -p void-cli -- --data-dir ~/.voidnet-b network bootstrap
cargo run -p void-cli -- --data-dir ~/.voidnet-b network diagnostics
cargo run -p void-cli -- --data-dir ~/.voidnet-b sessions
cargo run -p void-cli -- --data-dir ~/.voidnet-b chat rooms
```

Expected result:

- Bootstrap reconnect attempts increment.
- Active peers recover without restarting the runtime manually.
- Room membership and inbox state are restored.

## Relay Validation

Check relay visibility:

```sh
cargo run -p void-cli -- --data-dir ~/.voidnet-a network relays
```

Expected result:

- Reservation state is visible even before a relayed peer session exists.
- If relay-routed addresses using `/p2p-circuit` appear, the peer path shows relay.
- Event logs include `RelayReservationAttempted`, `RelayReservationAccepted`, and `RelayCircuitEstablished` when relevant.

## Event Validation

Inspect persisted operator events:

```sh
tail -n 100 ~/.voidnet-a/events.log
tail -n 100 ~/.voidnet-b/events.log
```

Look for lines such as:

```txt
[VOIDNET][NETWORK] BootstrapConnected peer=12D3K...
[VOIDNET][NETWORK] DirectSessionEstablished peer=12D3A...
[VOIDNET][NETWORK] ReachabilityChanged reachability=PUBLIC observed_address=/ip4/...
[VOIDNET][CHAT] RoomStateSynchronized room=operators ...
```

## Failure Notes

If WAN connectivity does not establish:

1. Confirm the bootstrap node is reachable on public UDP.
2. Confirm the bootstrap multiaddr contains the correct `/p2p/<peer-id>` suffix.
3. Confirm each machine is using a persistent data directory and not rotating identity.
4. Check `void network bootstrap` for degraded bootstrap state.
5. Check `void network peers` and `void network diagnostics` before changing runtime code.