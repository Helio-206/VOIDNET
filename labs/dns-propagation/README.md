# DNS Propagation Lab

Purpose: verify that a signed `.void` record published on one node propagates and resolves on another node.

## Start Two Nodes

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-dns-a --listen /ip4/127.0.0.1/udp/39501/quic-v1
cargo run -p void-node -- --data-dir /tmp/voidnet-dns-b --listen /ip4/127.0.0.1/udp/39502/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39501/quic-v1
```

## Publish A Route

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-a dns publish chat.void
```

## Resolve On Both Nodes

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-a dns resolve chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-b dns resolve chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-dns-b dns cache
```

Expected result:

- both nodes resolve `chat.void`
- `signature_state=verified`
- node B shows `source=MeshPropagation` in its cache view