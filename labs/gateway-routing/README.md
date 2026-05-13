# Gateway Routing Lab

This lab validates external route abstraction over VOID URIs.

## Goal

Confirm that a routed gateway URI creates an active gateway route with bridge context derived from the VOID path.

## Steps

1. Register a routed gateway:

```bash
cargo run -p void-cli -- gateway register docs.gateway.void --protocol https --capability gateway.http --capability gateway.external-routing --capability gateway.resource-fetch --external-route https://docs.example
```

2. Trust the gateway for runtime mounting:

```bash
cargo run -p void-cli -- gateway allow docs.gateway.void trusted
```

3. Open an external path through the runtime-native route:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://docs.gateway.void/reference/runtime
```

4. Read diagnostics:

```bash
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- Diagnostics expose a non-empty `runtime_shell_gateway_last_route`.
- `gateway_active_routes` increments after the open flow.
- The mounted surface bindings include the external target rooted at `https://docs.example/reference/runtime`.
