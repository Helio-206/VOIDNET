# Gateway Trust Lab

This lab validates trust policy enforcement for gateway mounts.

## Goal

Confirm that the runtime refuses untrusted gateways and records the denial path cleanly.

## Steps

1. Register a restricted gateway:

```bash
cargo run -p void-cli -- gateway register unsafe.gateway.void --protocol http --capability gateway.http --capability gateway.external-routing --external-route https://unsafe.example
```

2. Explicitly deny the gateway:

```bash
cargo run -p void-cli -- gateway deny unsafe.gateway.void untrusted
```

3. Attempt to open the gateway:

```bash
cargo run -p void-cli -- open void://unsafe.gateway.void
```

4. Read diagnostics and inspect the gateway:

```bash
cargo run -p void-cli -- gateway inspect unsafe.gateway.void
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- The open command fails with gateway trust denial.
- `gateway_bridge_failures` or denial-related diagnostics increase when the route cannot be mounted.
- The gateway inspection output shows the denied trust state and recorded permission history.
