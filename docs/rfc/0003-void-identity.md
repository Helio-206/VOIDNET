# RFC-0003 VOID Identity

Status: draft  
Objective: define deterministic peer identity and trust primitives  
Scope: key roots, peer id derivation, signatures, trust lifecycle, session keys, replay protection, and revocation

## Objective

VOID Identity defines how peers exist cryptographically and how protocol artifacts become attributable. It is infrastructure, not account management.

## Scope

Included:

- ed25519 identity roots.
- Deterministic peer id derivation.
- Signed payloads and future signed envelopes.
- x25519 session key negotiation.
- AES-GCM encrypted payload expectation.
- Trust lifecycle.
- Peer revocation.
- Replay protection concepts.

Excluded:

- Human profile systems.
- Email or password recovery.
- Global reputation.
- Social graph semantics.

## Protocol Assumptions

- A valid signature proves key possession.
- Authorization is scoped and separate from authentication.
- Identity material is persisted locally.
- Session keys rotate independently of long-term identity.
- Replay windows are enforced per session or operation class.

## Future Considerations

- Multibase or libp2p-compatible peer id encoding.
- Hardware-backed key storage.
- Signed migration statements for identity rotation.
- Trust graph propagation.
- Revocation distribution strategy.

