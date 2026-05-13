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