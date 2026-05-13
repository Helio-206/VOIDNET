# RFC-0002 VOIDDNS

Status: draft  
Objective: define decentralized `.void` name resolution  
Scope: signed records, local cache, distributed lookup, conflict retention, and service targets

## Objective

VOIDDNS resolves `.void` names into peer, content, or service targets without depending on centralized DNS. It must remain observable under conflict and safe under namespace poisoning attempts.

## Scope

Included:

- `.void` domain validation.
- Record target types.
- TTL behavior.
- Record sequence.
- Issuer identity.
- Signed record verification.
- Local cache.
- Distributed lookup assumptions.

Excluded:

- Permanent global name allocation.
- Tokenized namespace ownership.
- Browser presentation of names.

## Protocol Assumptions

- DNS records are signed by VOID identities.
- Bootstrap nodes are reachability hints, not trusted naming authorities.
- Records may conflict.
- Expired records must not be returned as authoritative.
- Distributed lookup may return multiple candidates.

## Future Considerations

- Delegation records.
- Revocation records.
- Conflict adjudication policy.
- DHT key format.
- Namespace audit traces.

