# Runtime Shell Lab

Purpose: validate that a published `void://` route mounts as an executable runtime surface and becomes visible in runtime diagnostics.

## Prepare A Node

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-runtime-shell --listen /ip4/127.0.0.1/udp/39801/quic-v1
```

In another terminal:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-shell dns publish chat.void
```

Wait for the node to process the queued DNS command, then run:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-shell open void://chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-shell runtime mounts
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-shell runtime sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-runtime-shell diagnostics
```

Expected result:

- `SurfaceMounted` is printed during `open`
- one active runtime mount appears
- one active runtime session appears
- diagnostics show non-zero runtime-shell mounts and sessions