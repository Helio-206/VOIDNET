# Network Recovery Lab

This lab validates reconnect resilience for bootstrap, relay reservation, and peer sessions.

## Scenario A: Relay Restart

1. Start the public relay node with `--relay-server`.
2. Start two private nodes against that bootstrap.
3. Kill the relay node abruptly.
4. Observe each private node:

```bash
cargo run -p void-cli -- --data-dir ./data/node-a network bootstrap
cargo run -p void-cli -- --data-dir ./data/node-a network relays
cargo run -p void-cli -- --data-dir ./data/node-a network diagnostics
```

5. Restart the relay node.

Expected:

- Bootstrap state transitions to `DEGRADED` during outage.
- Relay reservations and relay sessions disappear or degrade.
- Reservation and relay connectivity return after restart.

## Scenario B: Private Peer Restart

1. Keep relay online.
2. Restart one private node.
3. Verify the surviving peer eventually shows the recovered session.

Expected:

- Reconnect attempts increase.
- Session metadata is refreshed without deleting topology state.
- Direct upgrade attempts resume after the relayed path is restored.

## Scenario D: UDP Disruption

1. Keep all three nodes online.
2. Temporarily block UDP `7000` on one client or at the edge firewall.
3. Run:

```bash
scripts/wan/smoke-check.sh "$HOME/.voidnet/machine-a"
```

Expected:

- Relay reservation state degrades or fails clearly.
- `last_error` explains the disruption.
- After unblocking UDP, reservations and peer connectivity recover.

## Scenario C: Reachability Regression

1. Force one peer onto a stricter NAT or firewall.
2. Observe `network reachability` and `network sessions` again.

Expected:

- AutoNAT transitions are logged.
- Relay fallback remains usable even if direct upgrade regresses.