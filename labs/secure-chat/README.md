# Secure Chat Lab

Purpose: validate the full operator path from CLI command queue to encrypted delivery and persisted inbox state.

## Start Two Nodes

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-lab-a --listen /ip4/127.0.0.1/udp/39201/quic-v1
cargo run -p void-node -- --data-dir /tmp/voidnet-lab-b --listen /ip4/127.0.0.1/udp/39202/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39201/quic-v1
```

## Inspect Peer IDs

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-a identity --persistent
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-b identity --persistent
```

## Join A Shared Room

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-a chat join operators
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-b chat join operators
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-a chat rooms
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-b chat rooms
```

Expected result:

- both nodes mark `operators` as joined locally
- room membership snapshots show both peers as `ONLINE`

## Send A Direct Message

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-a chat send <peer-id-of-b> "hello secure mesh"
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-a chat sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-b chat inbox
```

Expected result:

- a negotiating session appears and then stabilises as `ESTABLISHED`
- the inbox on B contains the plaintext body
- event logs show `SessionNegotiationStarted`, `EncryptedSessionEstablished`, `PayloadVerified`, and `EncryptedMessageDelivered`

## Replay Check

Repeat the same command fast enough to inspect the event stream while the node is running:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-lab-a chat send <peer-id-of-b> "hello secure mesh"
```

Expected result:

- new messages with fresh nonces are accepted
- duplicated inbound envelopes are rejected by replay tracking if reintroduced through the mesh