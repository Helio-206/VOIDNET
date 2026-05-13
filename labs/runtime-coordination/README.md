# Runtime Coordination

Purpose: validate that nodes now exchange runtime-aware mesh state instead of exposing only transport reachability.

Run the two-node simulator:

```sh
bash labs/swarm-simulator/run-two-node.sh
```

Inspect the persisted runtime state:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-labs/swarm-simulator/a diagnostics
cargo run -p void-cli -- --data-dir /tmp/voidnet-labs/swarm-simulator/a runtime
cargo run -p void-cli -- --data-dir /tmp/voidnet-labs/swarm-simulator/a topology
```

What to look for:

- mesh state changes
- runtime readiness
- peer runtime discovery
- transport health updates
- persisted runtime snapshots in `topology.json`