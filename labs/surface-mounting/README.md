# Surface Mounting Lab

Purpose: validate multi-step route opening from VOIDDNS resolution into a mounted runtime surface.

## Publish And Open

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-surface-mount dns publish chat.void
cargo run -p void-node -- --data-dir /tmp/voidnet-surface-mount --exit-after-secs 3
cargo run -p void-cli -- --data-dir /tmp/voidnet-surface-mount open void://chat.void
```

## Inspect Runtime State

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-surface-mount runtime mounts
cargo run -p void-cli -- --data-dir /tmp/voidnet-surface-mount runtime registry
cargo run -p void-cli -- --data-dir /tmp/voidnet-surface-mount topology
```

Expected result:

- the route resolves into the `chat` runtime surface
- the registry shows `chat.void`
- topology includes runtime-shell mount and session visibility