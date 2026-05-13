# Gateway Fetch Lab

This lab validates fetch lifecycle and timeout handling.

## Goal

Confirm that the runtime treats outbound HTTP work as gateway bridge operations with observable lifecycle transitions.

## Steps

1. Mount a normal resource:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://example.gateway.void/api/health
```

2. Mount a slow resource to test timeout behavior:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://example.gateway.void/slow
```

3. Read diagnostics:

```bash
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- `BridgeSessionStarted`, `FetchDispatched`, `ResponseReceived`, and `ExternalResourceMounted` appear in the event stream for successful fetches.
- Slow or broken endpoints move the bridge into a failed state.
- Diagnostics expose `runtime_shell_gateway_bridge_failures` and the latest `runtime_shell_gateway_last_route`.
