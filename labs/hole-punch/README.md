# Hole Punch Validation Lab

This lab validates DCUtR direct upgrade attempts after a relayed session is established.

## Preconditions

- Follow the relay lab first.
- Ensure both private peers can reach the public relay.
- Keep `--relay-server` enabled only on the public node.

## Validation Steps

1. Bring up the relay node and both private nodes.
2. Wait for the private peers to discover each other through the relay.
3. Inspect the operator output:

```bash
cargo run -p void-cli -- --data-dir ./data/node-a network sessions
cargo run -p void-cli -- --data-dir ./data/node-a network diagnostics
```

4. Inspect transport events:

```bash
tail -n 80 ./data/node-a/events.log
tail -n 80 ./data/node-b/events.log
```

## Expected Sequence

- `RelaySessionEstablished`
- `HolePunchAttempt`
- `HolePunchSucceeded` or `HolePunchFailed`
- `DirectUpgradeSucceeded` or `DirectUpgradeFailed`

## Success Criteria

- `hole_punch_attempts` increments in diagnostics.
- `hole_punch_successes` increments when the direct upgrade lands.
- Session path changes from relay to direct once the direct connection becomes authoritative.

## Failure Criteria

- Relay stays active, but no direct upgrade result appears.
- Diagnostics show repeated hole punch attempts with zero successes.
- AutoNAT reachability remains `Private` or `Unknown` on both sides.