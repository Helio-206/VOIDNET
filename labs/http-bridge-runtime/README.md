# HTTP Bridge Runtime Lab

This lab validates the runtime-native HTTP bridge lifecycle.

## Goal

Confirm that a trusted gateway creates a bridge session, dispatches an HTTP fetch, persists a snapshot, and renders a response surface inside the runtime.

## Steps

1. Register a gateway mapped to a reachable HTTP origin:

```bash
cargo run -p void-cli -- gateway register example.gateway.void --protocol http --capability gateway.http --capability gateway.external-routing --capability gateway.resource-fetch --capability gateway.response-stream --external-base http://127.0.0.1:8080
```

2. Trust the gateway:

```bash
cargo run -p void-cli -- gateway allow example.gateway.void trusted
```

3. Mount the external resource through the runtime:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://example.gateway.void/status
```

4. Inspect diagnostics:

```bash
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- `runtime_shell_gateway_bridge_sessions` is non-zero.
- `runtime_shell_gateway_last_external_target` matches the mounted HTTP resource.
- `runtime_shell_gateway_last_fetch_latency_ms` and `runtime_shell_gateway_last_response_size` are populated.
- A snapshot JSON file appears under `data_dir/gateway/example.gateway.void/`.
