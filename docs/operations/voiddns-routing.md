# VOIDDNS Routing Operations

Scope: real distributed `.void` naming and `void://` service routing over the current VOIDNET mesh.

This slice adds:

- Ed25519-signed `.void` records
- local DNS cache persistence under `data_dir/dns/`
- TTL-aware expiration
- gossipsub propagation on `voidnet.dns.mesh.v1`
- duplicate suppression via record fingerprint event ids
- ownership conflict detection
- `void://` route resolution with target peer and runtime surface
- CLI tooling for publish, resolve, inspect, list, and cache views

## Record Model

Each propagated record now carries:

- `domain`
- `owner_peer_id`
- `target_peer_id`
- `runtime_surface`
- `capabilities`
- `created_at_unix_ms`
- `expires_at_unix_ms`
- `public_key`
- `signature`

Records are verified against the embedded public key and peer id before entering the local cache.

## Propagation Topic

The mesh-wide DNS topic is:

```text
voidnet.dns.mesh.v1
```

Nodes publish signed `DnsPropagationMessage` values on this topic when they accept a new local registration.

## CLI Surface

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns publish chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns resolve chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns list
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns cache
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns inspect chat.void
```

`dns publish` writes a local command into `data_dir/dns/commands/`. The running node signs the record, stores it, emits DNS events, and propagates it over gossipsub.

## Diagnostics

The transport event stream now exposes:

- `DnsRecordPublished`
- `DnsRecordUpdated`
- `DnsRecordExpired`
- `DnsRecordRejected`
- `DnsConflictDetected`
- `DnsResolutionSucceeded`
- `DnsResolutionFailed`

Topology diagnostics now include:

- cache entry count
- active record count
- conflict count
- local runtime registrations
- last DNS resolution latency

## Runtime Routing

The runtime now resolves `void://` authorities through VOIDDNS route resolution rather than returning raw unresolved service names.

Resolved route output includes:

- target peer id
- runtime surface
- capability list
- signature verification state
- resolution latency

## Conflict Rules

Current conflict behaviour is intentionally strict:

- invalid signatures are rejected
- expired records are rejected or purged
- the first active owner for a live domain remains authoritative
- competing live ownership for the same domain is stored as conflict state and surfaced in diagnostics

## Cache Layout

The node persists DNS state under:

```text
data_dir/dns/cache.json
data_dir/dns/commands/
```

With the default node dir, this maps to `~/.voidnet/dns/`.