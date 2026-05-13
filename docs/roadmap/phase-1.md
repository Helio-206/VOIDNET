# Phase 1 Roadmap: Network Foundation

Status: active draft

Phase 1 proves that VOIDNET can run as a sovereign peer-to-peer operating layer above existing internet transport. The target is not a demo network. The target is a minimal protocol substrate with autonomous nodes, deterministic identity, typed message flow, decentralized name resolution, runtime mediation, and a first application surface that exercises the whole stack.

VOIDNET is treated as layered infrastructure:

```text
VOIDNET
|-- Transport Layer
|-- Identity Layer
|-- Routing Layer
|-- DNS Layer
|-- Runtime Layer
|-- Distributed State Layer
|-- Application Layer
`-- Browser Surface Layer
```

Each layer owns a narrow responsibility boundary and communicates through typed events, signed envelopes, or explicit runtime calls. No layer is allowed to assume a central server, a trusted backend, or a permanent network topology.

## Milestone 1: Repository Foundation

Baseline deliverables:

- Workspace manifests.
- Core crate boundaries.
- CLI and node shells.
- Protocol, DNS, identity, and runtime docs.
- Initial SDK stubs.

Systems intent:

- Establish the repository as a protocol initiative with explicit ownership boundaries between transport, identity, DNS, runtime, storage, and applications.
- Treat app shells as protocol validation surfaces rather than product shells.
- Keep Rust responsible for network-critical paths and Go responsible for infrastructure tooling.
- Preserve minimalism without reducing architectural depth.

## Milestone 2: Transport Prototype

Baseline deliverables:

- Build libp2p swarm with QUIC transport.
- Add identify and ping behavior.
- Add bootstrap dialing.
- Emit typed `TransportEvent` values.
- Add integration test with two local nodes.

Systems intent:

- Establish autonomous peer identity propagation across the encrypted VOID transport mesh.
- Use libp2p and QUIC as the substrate for peer existence, not as a client/server tunnel.
- Model churn, reconnect storms, partial partitions, hostile peers, spoof attempts, and unstable latency as normal network conditions.
- Emit transport state as typed events so runtime, DNS, and applications never depend on raw swarm internals.
- Validate mesh formation through local multi-node tests before application features expand.

## Milestone 3: Identity and Secure Messaging

Baseline deliverables:

- Persist identities.
- Sign protocol envelopes.
- Verify signed peer messages.
- Add x25519 key exchange.
- Encrypt private payloads with AES-GCM.

Systems intent:

- Treat identity as infrastructure, not account state.
- Anchor every peer in deterministic cryptographic identity derived from public key material.
- Bind protocol envelopes to signatures, nonces, sequence windows, and replay protection.
- Establish rotating x25519 session keys for payload secrecy while preserving long-term ed25519 identity roots.
- Carry trust into runtime permissions, DNS records, room membership, and distributed state snapshots.

## Milestone 4: VOID DNS Prototype

Baseline deliverables:

- Local cache with TTL.
- Signed `.void` records.
- DHT-backed lookup.
- Bootstrap-published seed records.
- Conflict diagnostics.

Systems intent:

- Make `.void` resolution a decentralized namespace protocol, not a lookup table.
- Store signed records with TTL, sequence, issuer identity, target type, and conflict metadata.
- Support peer, content, and service targets without assuming that names are globally honest.
- Treat namespace poisoning and fake bootstrap records as first-order threats.
- Keep record conflict behavior observable instead of silently overwriting distributed disagreement.

## Milestone 5: VOIDChat

Baseline deliverables:

- Identity-based users.
- Decentralized rooms.
- Direct peer messaging.
- Encrypted message bodies.
- Runtime integration path shared with VOIDBrowser.

Systems intent:

- Use VOIDChat as the first protocol exercise across identity, DNS, transport, encryption, runtime, and app lifecycle.
- Model rooms as distributed communication surfaces anchored by identity and signed state.
- Avoid treating chat as the product. Chat is the first pressure test of the network substrate.
- Route all app traffic through VOID Protocol frames and runtime-mediated capabilities.

## Milestone 6: VOIDBrowser Runtime Shell

Baseline deliverables:

- Minimal native shell.
- `void://` navigation.
- `.void` resolution.
- VOID UI parser.
- Permission prompts.
- App isolation boundary.

Systems intent:

- Build VOIDBrowser as a runtime surface for distributed protocol applications, not as a Chrome-shaped browser clone.
- Mount VOID UI documents as isolated app surfaces with capability-mediated access to identity, storage, streams, and network calls.
- Treat `void://` navigation as runtime dispatch into DNS, routing, protocol, and app lifecycle machinery.
- Keep the browser surface dark, minimal, quiet, and engineered around the substrate rather than around web page conventions.

## System Layer Model

The full layer contract is defined in [System Layer Model](../architecture/system-layer-model.md).

- Transport Layer: encrypted mesh connectivity, peer churn handling, swarm events, and QUIC-backed streams.
- Identity Layer: deterministic peer roots, signatures, trust lifecycle, session key rotation, and revocation.
- Routing Layer: envelope forwarding, path selection, hop limits, partition awareness, and propagation boundaries.
- DNS Layer: signed `.void` records, local cache, distributed lookup, conflict diagnostics, and namespace trust.
- Runtime Layer: capability execution, permission mediation, app mounting, and sandbox isolation.
- Distributed State Layer: replicated state, partial synchronization, signed snapshots, deterministic recovery, and conflict-aware merges.
- Application Layer: protocol-native applications that consume runtime capabilities instead of assuming backend APIs.
- Browser Surface Layer: native presentation surface for `void://` apps, VOID UI documents, permission prompts, and app lifecycle controls.

## Transport Philosophy

VOIDNET transport is not client/server networking. Nodes are sovereign participants in a distributed encrypted mesh. A node exists before it is useful to another node, and it remains a valid participant even when the mesh is partitioned.

The transport layer must survive peer churn, unstable networks, reconnect storms, partial partitions, hostile peers, spoof attempts, and unreliable latency. Transport work therefore prioritizes event-driven networking, swarm resilience, distributed propagation, explicit backpressure, and typed failure states over request/response convenience.

Reference: [Transport Philosophy](../architecture/transport-philosophy.md).

## Identity Philosophy

Identity is infrastructure. VOID identity roots define peer existence, message authorship, runtime permissions, DNS ownership, and distributed state authority. Identity does not imply human profile data or account recovery.

Phase 1 identity work includes deterministic peer identity, signed protocol envelopes, trust establishment, capability negotiation, distributed trust propagation, runtime permission anchoring, peer revocation, rotating session keys, persistence, and replay protection concepts.

Reference: [Identity](../architecture/identity.md).

## Runtime Philosophy

The VOID runtime is not a browser runtime. Applications are distributed protocol surfaces mounted inside an execution layer that mediates capabilities, isolates state, and dispatches communication through VOID Protocol.

The runtime should feel closer to a distributed operating layer than a web runtime. It owns app lifecycle management, sandbox boundaries, permission negotiation, runtime-native communication, and distributed app execution.

Reference: [Runtime Philosophy](../architecture/runtime-philosophy.md).

## Distributed Event Bus

VOIDNET components communicate through typed events. The event bus is not a global message queue. It is a boundary discipline for observing, routing, and auditing asynchronous state transitions across transport, identity, DNS, runtime, and distributed state.

Initial event vocabulary:

- `PeerDiscovered`
- `PeerAuthenticated`
- `PeerDisconnected`
- `DomainResolved`
- `RuntimeMounted`
- `RuntimeIsolated`
- `SessionEncrypted`
- `MeshPartitionDetected`
- `IdentityRevoked`
- `DistributedStateSynced`

Reference: [Distributed Event Bus](../architecture/distributed-event-bus.md).

## Distributed State Layer

VOIDNET requires future distributed persistence for rooms, names, app manifests, runtime metadata, and replicated application state. This layer is not blockchain. It does not assume global consensus, token economics, mining, or a single append-only world ledger.

The target is replicated state with partial synchronization, signed state snapshots, deterministic recovery, conflict-aware merges, and distributed memory concepts bounded by identity and application authority.

Reference: [Distributed State Layer](../architecture/distributed-state.md).

## Threat Model

The network assumes adversarial conditions:

- Malicious peers.
- Replay attacks.
- Namespace poisoning.
- Invalid signatures.
- Mesh flooding.
- Fake bootstrap nodes.
- Identity spoofing.
- Partition attacks.

Mitigation philosophy is strict: authenticate identity, verify signatures, bound propagation, rate-limit hostile traffic, retain conflict evidence, quarantine suspicious peers, and keep trust decisions explicit.

Reference: [Threat Model](../architecture/threat-model.md).

## RFC System

VOIDNET protocol decisions are formalized through RFCs:

- [RFC-0001 VOID Protocol](../rfc/0001-void-protocol.md)
- [RFC-0002 VOIDDNS](../rfc/0002-voiddns.md)
- [RFC-0003 VOID Identity](../rfc/0003-void-identity.md)
- [RFC-0004 Runtime Permissions](../rfc/0004-runtime-permissions.md)
- [RFC-0005 Distributed State](../rfc/0005-distributed-state.md)
- [RFC-0006 Transport Layer](../rfc/0006-transport-layer.md)
- [RFC-0007 Event Bus](../rfc/0007-event-bus.md)

## Completion Criteria

Two local nodes can bootstrap, resolve `chat.void`, exchange signed encrypted messages, and open the same VOIDChat surface through the runtime path.

The deeper completion condition is stronger: two nodes must prove that VOIDNET can preserve identity, routing, naming, encrypted communication, runtime permission boundaries, and application state transitions without depending on a central server or web platform assumptions.
