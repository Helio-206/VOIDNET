# RFC-0004 Runtime Permissions

Status: draft  
Objective: define capability-based permissions for VOID Runtime  
Scope: app authority, permission grants, runtime isolation, app lifecycle, and audit events

## Objective

Runtime permissions restrict distributed applications to explicit capabilities. The runtime mediates identity signing, storage, network access, state synchronization, and streams.

## Scope

Included:

- Capability vocabulary.
- Permission grant structure.
- Permission denial events.
- App isolation boundaries.
- Runtime mount lifecycle.
- Permission revocation.

Excluded:

- Browser UI design.
- App marketplace policy.
- Host OS sandbox implementation.

## Protocol Assumptions

- Apps are untrusted by default.
- Permissions are scoped to app identity, issuer identity, capability, and namespace.
- Browser approval is not enforcement.
- Identity signing must be mediated by runtime.
- Permission changes emit events.

## Future Considerations

- Capability leases.
- Signed app manifests.
- Permission templates.
- Cross-device runtime recovery.
- Runtime audit log format.

