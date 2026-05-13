# Architecture Philosophy

Status: draft  
Scope: project posture and engineering constraints

VOIDNET is an experimental operating substrate above the internet. It is built as a protocol ecosystem, not as an application platform wrapped around a backend. Its first responsibility is to make distributed execution, identity, naming, routing, and state explicit.

## Position

VOIDNET is:

- A private internet-like ecosystem.
- A peer-to-peer network.
- A custom protocol stack.
- A decentralized application runtime.
- A browser, protocol, and node ecosystem.

VOIDNET is not:

- A startup web app.
- A SaaS platform.
- A CRUD backend.
- A blockchain clone.
- A generic libp2p demo.

## Engineering Posture

VOIDNET should feel cold, minimal, and engineered. The project should expose its operating surfaces:

- Wire protocol.
- Identity roots.
- Trust transitions.
- Runtime capabilities.
- Distributed state boundaries.
- Failure states.
- Threat model.

The codebase should avoid theatrical complexity. Depth should come from correct boundaries, typed protocol surfaces, explicit trust, and recoverable failure behavior.

## Operating Assumptions

- The network is unstable.
- Peers are not inherently trustworthy.
- Names can be contested.
- State can diverge.
- Bootstrap can be hostile.
- Applications are untrusted until capability-scoped.
- Runtime authority must be mediated.
- Observability is part of correctness.

## Design Constraints

- Prefer typed frames over ad hoc payload conventions.
- Prefer local authority with distributed verification over central coordination.
- Prefer scoped trust over global trust.
- Prefer observable conflict over silent overwrite.
- Prefer degraded operation over collapse.
- Prefer protocol documents before broad feature expansion.

## Repository Atmosphere

The repository should read like advanced infrastructure software:

- RFCs define protocol commitments.
- Architecture docs define layer boundaries.
- Roadmaps define systems milestones.
- Core crates own narrow responsibilities.
- Apps validate the substrate rather than define it.

VOIDNET should remain serious without becoming corporate, minimal without becoming shallow, and futuristic without losing mechanical precision.

