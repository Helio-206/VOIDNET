# Encrypted Sessions

Purpose: validate real end-to-end session negotiation and encrypted direct payload delivery.

## Setup

Start node A:

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-chat-a --listen /ip4/127.0.0.1/udp/39101/quic-v1
```

Start node B:

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-chat-b --listen /ip4/127.0.0.1/udp/39102/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39101/quic-v1
```

Discover B's peer id:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-chat-b identity --persistent
```

Queue a direct encrypted message from A to B:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-chat-a chat send <peer-id-of-b> "mesh says hello"
```

Inspect the resulting session and inbox state:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-chat-a chat sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-chat-b chat sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-chat-b chat inbox
```

What to verify:

- both nodes persist the same `session_id`
- session state moves to `ESTABLISHED`
- transport shows `GOSSIPSUB-DIRECT`
- cipher shows `aes-256-gcm`
- B receives the plaintext body only after local decryption

Replay protection is now enforced on signed offers, acks, room events, and direct envelopes.
*** Add File: /home/helio/HEr/VOID/docs/operations/secure-peer-communication.md
# Secure Peer Communication

Scope: actual end-to-end peer messaging over the current VOIDNET mesh without redesigning transport.

This slice adds a real operator path for secure communication:

- X25519 session negotiation
- AES-256-GCM encrypted payloads
- Ed25519-signed control and data envelopes
- replay-window enforcement on inbound control/data messages
- file-backed local chat command queue for CLI-to-node interaction
- persisted inbox, room, and session state under the node data directory

## Data Flow

1. `void-cli chat send <peer_id> "message"` writes a command into `data_dir/chat/commands/`.
2. The running node drains queued commands on a short interval.
3. If no active session exists, the sender emits a signed `SessionOffer` on the recipient direct topic.
4. The recipient verifies the signature, checks replay state, generates its own X25519 key, derives the shared key, and emits a signed `SessionAck`.
5. Both peers derive the same AES key from the X25519 shared secret plus `session_id`.
6. The sender encrypts the message payload with AES-256-GCM and publishes a signed direct envelope on the recipient direct topic.
7. The recipient verifies the envelope, rejects replays, decrypts the payload, and appends the plaintext to the persisted inbox.

## Topics And State

Current topics:

- `voidnet.runtime.mesh.v1`
- `void.chat.peer.<peer_id>`
- `void.chat.room.<room>`

Current persisted state under `data_dir/chat/`:

- `commands/`: CLI-to-node work queue
- `inbox.json`: decrypted inbound direct messages
- `sessions.json`: active or negotiating chat sessions
- `rooms.json`: joined rooms and observed membership

## CLI Surface

Direct messaging and room coordination now flow through these commands:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat peers
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat send <peer-id> "hello"
cargo run -p void-cli -- --data-dir /tmp/voidnet-b chat inbox
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat join operators
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat rooms
```

## Event Surface

The node now emits additional operational events for the chat layer:

- `SessionNegotiationStarted`
- `EncryptedSessionEstablished`
- `PayloadVerified`
- `EncryptedMessageDelivered`
- `InvalidSignatureRejected`
- `ReplayRejected`
- `DecryptionFailed`
- `RoomMembershipChanged`

These events are emitted through the existing transport event logger so secure communication can be diagnosed from the same runtime stream.

## Current Limits

Implemented now:

- direct encrypted peer messaging
- room membership propagation and persistence
- session tracking with `peer_id`, `session_id`, `established_at`, `last_activity`, and transport state

Not implemented yet:

- encrypted room payload broadcast
- session rekeying and expiration policy
- durable session recovery after node restart
- delivery receipts or acked application-level sequencing
*** Add File: /home/helio/HEr/VOID/labs/secure-chat/README.md
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