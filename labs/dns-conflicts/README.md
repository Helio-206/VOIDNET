# DNS Conflicts Lab

Purpose: verify that competing live ownership claims are detected and preserved as conflict state instead of silently replacing the active owner.

## Start Two Nodes

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-conflict-a --listen /ip4/127.0.0.1/udp/39601/quic-v1
cargo run -p void-node -- --data-dir /tmp/voidnet-conflict-b --listen /ip4/127.0.0.1/udp/39602/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39601/quic-v1
```

## Publish The Same Domain From Both Nodes

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-conflict-a dns publish vault.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-conflict-b dns publish vault.void
```

## Inspect Conflict State

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-conflict-a dns inspect vault.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-conflict-b dns cache
cargo run -p void-cli -- --data-dir /tmp/voidnet-conflict-a diagnostics
```

Expected result:

- one active owner remains authoritative for `vault.void`
- competing ownership appears under `conflict_state=conflicted`
- diagnostics show a non-zero DNS conflict count