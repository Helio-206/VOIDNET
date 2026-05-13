# HTTP Bridge Lab

This lab validates the foundation for request, response, and stream lifecycle metadata without implementing full proxying.

## Goal

Confirm that HTTP-oriented gateway capabilities can be registered and surfaced through the runtime bridge model.

## Steps

1. Register an HTTP bridge gateway:

```bash
cargo run -p void-cli -- gateway register bridge.gateway.void --protocol http --capability gateway.http --capability gateway.resource-fetch --capability gateway.response-stream --capability gateway.external-routing --external-route https://httpbin.org
```

2. Trust the gateway:

```bash
cargo run -p void-cli -- gateway allow bridge.gateway.void trusted
```

3. Mount a bridge route:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://bridge.gateway.void/anything/runtime
```

4. Inspect the gateway and diagnostics:

```bash
cargo run -p void-cli -- gateway inspect bridge.gateway.void
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- The runtime mounts a gateway surface with bridge state visible.
- Diagnostics expose gateway route and bridge counters.
- No full HTTP proxying is required; only bridge metadata, trust, and route lifecycle should be exercised.
