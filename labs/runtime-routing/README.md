# Runtime Routing Lab

Purpose: validate that `void://` authorities resolve into runtime-linked service metadata rather than bare names.

## Publish A Runtime Surface

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-routing --listen /ip4/127.0.0.1/udp/39701/quic-v1
cargo run -p void-cli -- --data-dir /tmp/voidnet-routing dns publish room.core.void --surface room.core --capability service/room.core --capability routing/void-uri
```

## Inspect The Published Route

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-routing dns inspect room.core.void
cargo run -p void-cli -- uri void://room.core.void/rooms/main
```

Expected result:

- the DNS inspection shows `runtime_surface=room.core`
- capabilities include the routing capability
- the `void://` URI authority matches the published `.void` domain that the runtime can resolve