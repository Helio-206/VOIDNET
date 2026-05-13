# RFC-0006 Transport Layer

Status: draft  
Objective: define VOIDNET transport behavior over libp2p and QUIC  
Scope: encrypted mesh connectivity, swarm events, peer churn, reconnect behavior, and transport failure states

## Objective

The Transport Layer establishes encrypted peer connectivity and exposes typed events to the rest of VOIDNET. It is not a client/server channel.

## Scope

Included:

- libp2p swarm usage.
- QUIC transport.
- Listen and dial behavior.
- Bootstrap dialing.
- Identify and ping behavior.
- Peer connection events.
- Backoff and reconnect behavior.
- Partition suspicion events.

Excluded:

- Application payload semantics.
- DNS ownership.
- Runtime capability policy.

## Protocol Assumptions

- Nodes are autonomous participants.
- Addresses are not identity.
- Bootstrap nodes are untrusted until authenticated.
- Transport can degrade without invalidating identity.
- Events must be bounded and typed.

## Future Considerations

- Peer scoring.
- Quarantine integration.
- Adaptive stream scheduling.
- Transport metrics.
- Partition-healing heuristics.

