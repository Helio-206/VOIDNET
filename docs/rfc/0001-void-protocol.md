# RFC-0001 VOID Protocol

Status: draft  
Objective: define the typed application-layer protocol for VOIDNET  
Scope: `void://` URI handling, protocol envelopes, frame taxonomy, streams, app calls, content requests, and error frames

## Objective

VOID Protocol provides the strongly typed message surface shared by nodes, runtime, browser, DNS, state, and applications. It is the common language above transport and below app behavior.

## Scope

Included:

- Protocol versioning.
- `void://` URI authority and path model.
- Envelope structure.
- Peer messaging.
- Routing frames.
- Content request and chunk frames.
- Stream frames.
- App calls.
- VOID UI document transport.
- Protocol error frames.

Excluded:

- Transport security implementation.
- DNS record ownership.
- Runtime permission policy.
- Distributed state merge policy.

## Protocol Assumptions

- Transport provides encrypted QUIC sessions through libp2p.
- Identity provides signatures and peer id verification.
- Frames are bounded in size.
- Unknown major versions are rejected.
- Unknown minor version fields require a future canonical binary compatibility rule.

## Future Considerations

- Canonical binary layout independent of Rust bincode.
- Signed envelopes.
- Replay protection fields.
- Capability-aware frame admission.
- Formal error code registry.

