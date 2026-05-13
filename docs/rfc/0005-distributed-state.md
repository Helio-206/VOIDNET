# RFC-0005 Distributed State

Status: draft  
Objective: define future replicated state semantics for VOIDNET  
Scope: snapshots, deltas, partial synchronization, conflict-aware merges, and deterministic recovery

## Objective

Distributed State provides scoped persistence for applications, DNS metadata, runtime metadata, revocation sets, and room state. It is not blockchain and does not require global consensus.

## Scope

Included:

- Namespace-scoped state.
- Signed snapshots.
- Signed deltas.
- Partial synchronization.
- Conflict retention.
- Deterministic recovery.
- Merge policy hooks.

Excluded:

- Mining.
- Token economics.
- Global ledger semantics.
- Single total ordering across all namespaces.

## Protocol Assumptions

- State issuers are VOID identities.
- Nodes may be offline or partitioned.
- Synchronization may be partial.
- Conflicts are preserved as evidence.
- Recovery uses trusted roots, revocations, snapshots, deltas, and merge policy.

## Future Considerations

- CRDT support for selected namespaces.
- Snapshot compaction.
- State availability hints.
- Multi-peer recovery.
- Verifiable garbage collection.

