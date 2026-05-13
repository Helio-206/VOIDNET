# Resource Mounting Lab

This lab validates runtime-native external resource mounting.

## Goal

Confirm that `void://*.gateway.void` becomes a mountable external bridge layer rather than a raw curl-style fetch.

## Steps

1. Register and trust a gateway:

```bash
cargo run -p void-cli -- gateway register docs.gateway.void --protocol https --capability gateway.http --capability gateway.external-routing --capability gateway.resource-fetch --capability gateway.response-stream --external-base https://docs.example
cargo run -p void-cli -- gateway allow docs.gateway.void trusted
```

2. Mount a nested external resource path:

```bash
printf 'quit\n' | cargo run -p void-cli -- open void://docs.gateway.void/github/openai
```

3. Inspect runtime diagnostics:

```bash
cargo run -p void-cli -- diagnostics
```

## Expected Signals

- The mounted runtime surface stays `gateway:*` while rendering the external response.
- `runtime_shell_gateway_last_external_target` reflects the resolved upstream resource.
- `runtime_shell_gateway_last_bridge_state` and `runtime_shell_gateway_last_cache_state` are populated.
- Snapshot persistence under `data_dir/gateway/` proves resource mounting created runtime-native state.
