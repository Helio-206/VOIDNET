# WAN Relay Lab

This lab validates real relay reservation fallback across multiple machines and a public VPS relay/bootstrap node.

## Validation Topology

- One public VPS node with persistent identity.
- Machine A on network A.
- Machine B on network B.
- Shared bootstrap multiaddr derived from the VPS peer id.

## Firewall Requirements

On the VPS, open UDP port `7000` in both the provider security group and the host firewall.

Example with `ufw`:

```bash
sudo ufw allow 7000/udp
sudo ufw status
```

If your provider has an external firewall, add the same rule there before testing.

## 1. Start the Public Bootstrap/Relay Node

Use the helper script:

```bash
scripts/wan/start-bootstrap-relay.sh
```

This script:

- uses `~/.voidnet/bootstrap` by default,
- listens on `/ip4/0.0.0.0/udp/7000/quic-v1`,
- enables `--relay-server`,
- prints `peer_id=...`,
- prints a public bootstrap multiaddr template,
- prints operator diagnostics commands.

## 2. Capture the Public IP and Bootstrap Multiaddr

From the VPS:

```bash
curl -fsS https://api.ipify.org
cargo run -p void-cli -- --data-dir ~/.voidnet/bootstrap identity --persistent
```

Build the bootstrap multiaddr:

```text
/ip4/<VPS_PUBLIC_IP>/udp/7000/quic-v1/p2p/<BOOTSTRAP_PEER_ID>
```

## 3. Share `bootstrap.toml` with Clients

Client file contents:

```toml
bootstrap_nodes = [
  "/ip4/<VPS_PUBLIC_IP>/udp/7000/quic-v1/p2p/<BOOTSTRAP_PEER_ID>"
]
```

## 4. Start Machine A

```bash
scripts/wan/connect-client.sh \
  "/ip4/<VPS_PUBLIC_IP>/udp/7000/quic-v1/p2p/<BOOTSTRAP_PEER_ID>" \
  "$HOME/.voidnet/machine-a" \
  0
```

## 5. Start Machine B

```bash
scripts/wan/connect-client.sh \
  "/ip4/<VPS_PUBLIC_IP>/udp/7000/quic-v1/p2p/<BOOTSTRAP_PEER_ID>" \
  "$HOME/.voidnet/machine-b" \
  0
```

Prefer a different ISP, hotspot, or site for Machine B.

## 6. Run Operator Smoke Checks

On each client:

```bash
scripts/wan/smoke-check.sh "$HOME/.voidnet/machine-a"
```

Expected WAN indicators:

- `bootstrap_connected` is greater than zero.
- `reachability` shows `PUBLIC`, `PRIVATE`, or `UNKNOWN` with `nat_detail`.
- `relay_reservations` becomes non-zero when a reservation is accepted.
- `network relays` shows reservation lines even before a relayed peer session exists.
- `network sessions` shows relay peer, hole punch attempts, direct upgrade attempts, and last error.

## 7. Event Validation

Inspect persisted events:

```bash
tail -n 120 "$HOME/.voidnet/machine-a/events.log"
tail -n 120 "$HOME/.voidnet/machine-b/events.log"
```

Look for:

- `RelayReservationAttempted`
- `RelayReservationAccepted`
- `RelayReservationFailed`
- `RelayCircuitEstablished`
- `RelayFallbackActivated`

## 8. Failure Drill

- Stop Machine B.
- Confirm Machine A surfaces degraded peer or relay state.
- Start Machine B again.
- Confirm reservation is re-acquired and relay session returns without deleting local data.