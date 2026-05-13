# Threat Model

Status: draft  
Scope: Phase 1 security threats and mitigation posture

VOIDNET assumes adversarial network conditions. Peer-to-peer systems fail when trust is implicit, propagation is unbounded, or identity is treated as display metadata. Phase 1 security work keeps trust explicit and failures observable.

## Threats

### Malicious Peers

Threat:

- Peers may send malformed frames, lie about capabilities, flood streams, advertise false routes, or serve invalid state.

Mitigation philosophy:

- Verify all protocol frames.
- Bound frame size and queue depth.
- Score and quarantine hostile peers.
- Keep application payloads behind runtime mediation.

### Replay Attacks

Threat:

- A valid signed message may be captured and replayed later to repeat an operation.

Mitigation philosophy:

- Add envelope nonces.
- Track sequence windows.
- Bind messages to sessions and stream ids.
- Reject duplicate signed operations.

### Namespace Poisoning

Threat:

- A peer may publish false `.void` records or conflicting records for a domain.

Mitigation philosophy:

- Require signed records.
- Validate issuer and sequence.
- Retain conflict sets.
- Avoid silent overwrite of contested names.

### Invalid Signatures

Threat:

- A peer may forge identity material, mutate payloads, or provide malformed signatures.

Mitigation philosophy:

- Derive peer id from public key material.
- Verify signatures before trust transitions.
- Emit invalid-signature events for observability.
- Quarantine repeated offenders.

### Mesh Flooding

Threat:

- A peer may flood propagation paths with envelopes, DNS requests, or state deltas.

Mitigation philosophy:

- Apply rate limits and backpressure.
- Enforce hop limits.
- Use event-class propagation boundaries.
- Drop or quarantine noisy peers.

### Fake Bootstrap Nodes

Threat:

- A bootstrap address may point to a hostile peer that advertises false topology or namespace state.

Mitigation philosophy:

- Treat bootstrap as reachability, not trust.
- Verify every peer identity independently.
- Require signed DNS and state records.
- Support multiple bootstrap paths.

### Identity Spoofing

Threat:

- A peer may claim another peer id or reuse stale identity material.

Mitigation philosophy:

- Derive peer id locally from public key.
- Bind Hello frames and envelopes to signatures.
- Reject mismatched peer id and public key pairs.
- Use session key rotation for encrypted channels.

### Partition Attacks

Threat:

- An attacker may isolate a node or group of nodes and feed stale routes, stale namespace records, or manipulated state.

Mitigation philosophy:

- Detect partition evidence.
- Track record TTL and sequence.
- Recover through multiple peer neighborhoods.
- Preserve conflict evidence after healing.

## Security Posture

VOIDNET does not attempt to eliminate hostile conditions. It narrows authority, verifies claims, bounds propagation, makes disagreement visible, and allows nodes to degrade rather than collapse.

