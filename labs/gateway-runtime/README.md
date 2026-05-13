# Gateway Runtime Lab

This lab validates that gateways behave as runtime-native executable surfaces.

## Goal

Confirm that a gateway can be registered, projected into runtime topology, mounted through `void://`, and surfaced by diagnostics.

## Steps

1. Register the builtin gateway foundation:

```bash
cargo run -p void-cli -- gateway register local.gateway.void --protocol http --protocol https --capability gateway.http --capability gateway.external-routing --capability gateway.response-stream --external-route https://localhost
```

2. Inspect the registry projection:

```bash
cargo run -p void-cli -- gateway list
cargo run -p void-cli -- gateway inspect local.gateway.void
```

3. Trust the gateway for runtime mounting:

```bash
cargo run -p void-cli -- gateway allow local.gateway.void trusted
```

4. Mount the gateway as a runtime surface:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://local.gateway.void
```

5. Confirm topology and diagnostics show gateway counters:

```bash
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- `gateway_registrations` is non-zero.
- Diagnostics include `runtime_shell_gateway_registrations` and `runtime_shell_gateway_mounts`.
- Opening `void://local.gateway.void` renders the builtin gateway surface instead of failing surface resolution.
