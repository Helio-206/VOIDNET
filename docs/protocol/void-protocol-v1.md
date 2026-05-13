# VOID Protocol v1

Status: draft  
Scheme: `void://`  
Layer: application protocol above libp2p/QUIC  
Primary implementation: `core/protocol`

## Goals

VOID Protocol is the minimal strongly typed protocol spoken by VOIDNET nodes and apps. It supports peer messaging, routing, content requests, streams, and app communication without copying ordinary HTTP semantics.

Design requirements:

- Binary-efficient framing.
- Explicit protocol versioning.
- Strong frame types.
- Extensible application calls.
- Safe default limits.
- Transport independence above libp2p streams.

## URI Model

VOID URIs use the `void://` scheme.

```text
void://chat.void
void://chat.void/rooms/main
void://core.void/identity
void://core/connect
```

The authority may be:

- A `.void` domain resolved by VOID DNS.
- A reserved runtime authority such as `core`.
- A future peer-scoped authority.

## Envelope

All messages are carried inside a protocol envelope.

```text
Envelope {
  version: ProtocolVersion,
  stream_id: u64,
  ttl: u8,
  frame: Frame
}
```

`version` is currently `1.0`. `stream_id` groups request, response, and stream chunks. `ttl` limits routing hops. `frame` is one of the typed v1 frame variants.

The reference implementation currently serializes envelopes with Serde plus bincode and enforces a 1 MiB frame limit. A later RFC should define canonical byte ordering, length prefixes, and compatibility rules for non-Rust implementations.

## Frame Types

### Hello

Used during application protocol negotiation after transport connection.

```text
Hello {
  peer_id,
  agent,
  supported_versions
}
```

### PeerMessage

Used for direct messages, room messages, and system messages.

```text
PeerMessage {
  from,
  to,
  kind,
  content_type,
  payload
}
```

Payloads should be encrypted before this layer when carrying private data.

### Route

Used by nodes to describe the intended destination and path.

```text
Route {
  destination,
  next_hop,
  path
}
```

### ContentRequest and ContentChunk

Used for named content retrieval through `void://` URIs.

```text
ContentRequest {
  uri,
  accept
}

ContentChunk {
  uri,
  sequence,
  final_chunk,
  bytes
}
```

### StreamOpen, StreamData, StreamClose

Used for long-lived flows such as subscriptions, app channels, and duplex communication.

```text
StreamOpen {
  uri,
  mode
}

StreamData {
  sequence,
  bytes,
  final_chunk
}

StreamClose {
  reason
}
```

### AppCall

Used for application-to-application or runtime-to-application calls.

```text
AppCall {
  uri,
  method,
  payload
}
```

### UiDocument

Carries a VOID UI document for browser/runtime rendering.

```text
UiDocument {
  uri,
  source
}
```

## Error Model

```text
ProtocolError {
  code: u16,
  message: string
}
```

Initial code ranges:

- `1000-1999`: protocol and version errors.
- `2000-2999`: identity and signature errors.
- `3000-3999`: DNS and routing errors.
- `4000-4999`: app/runtime errors.
- `5000-5999`: transport-backed failures.

## Security Expectations

VOID Protocol does not replace transport security. It assumes QUIC transport encryption from libp2p and adds message signatures and payload encryption where application semantics require end-to-end confidentiality.

Private messages should use:

- ed25519 identity keys for signing.
- x25519-derived shared secrets for session establishment.
- AES-GCM for payload encryption.

## Compatibility

Nodes must reject unknown major versions. Nodes may ignore unknown minor-version frame fields only after the canonical binary layout RFC defines field-level extension behavior.

