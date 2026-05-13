# Latency Tests

Purpose: observe ping RTT and transport health.

Run two nodes and inspect:

```sh
cargo run -p void-cli -- peers
```

Expected output includes peer id, lifecycle-derived state, latency, and transport.

