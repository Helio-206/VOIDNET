# Live Mesh Observability

Scope: operator-facing runtime and mesh visibility for the current VOIDNET implementation.

This document covers the behaviour that is now implemented in the transport/runtime substrate:

- persisted peer runtime state
- live mesh state transitions
- runtime capability visibility
- encrypted session visibility
- terminal diagnostics

## What Exists Now

Each node now persists runtime-aware peer state into the local topology snapshot.

Tracked fields include:

- `peer_id`
- peer connection state
- runtime version
- runtime lifecycle state
- uptime seconds
- capability list
- transport health
- runtime readiness
- session encryption state
- reconnect attempts
- last error

Topology persistence remains JSON-backed and is still written to the node data directory as `topology.json`.

## Runtime Mesh Announcements

Nodes now publish runtime mesh announcements over the mesh topic:

```text
voidnet.runtime.mesh.v1
```

Announcements currently carry:

- `peer_id`
- runtime snapshot
- optional latency
- encrypted session presence

This allows nodes to ingest remote runtime state and evolve local topology from more than raw transport events.

## Event Surface

The local event bus now includes runtime-native and mesh-native signals such as:

- `RuntimeMounted`
- `RuntimeReady`
- `RuntimeShutdown`
- `CapabilityGranted`
- `CapabilityRejected`
- `PeerRuntimeDiscovered`
- `MeshStateChanged`
- `PartitionDetected`
- `PartitionRecovered`
- `EncryptedSessionEstablished`

These events are emitted into the existing event logger and are intended to become the coordination spine for higher runtime layers.

## CLI Commands

The CLI now exposes the following operational views:

```sh
cargo run -p void-cli -- peers
cargo run -p void-cli -- topology
cargo run -p void-cli -- diagnostics
cargo run -p void-cli -- runtime
cargo run -p void-cli -- sessions
```

Expected uses:

- `peers`: peer table with runtime readiness and transport health.
- `topology`: live-oriented ASCII mesh view.
- `diagnostics`: aggregate health summary for the local node snapshot.
- `runtime`: runtime metadata for local and remote peers.
- `sessions`: encrypted-session and reconnect visibility.

## Running A Simple Coordination Check

Start one node:

```sh
cargo run -p void-node
```

Start a second node with bootstrap:

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-b --listen /ip4/127.0.0.1/udp/39102/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39101/quic-v1
```

Inspect the resulting mesh state:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-b diagnostics
cargo run -p void-cli -- --data-dir /tmp/voidnet-b runtime
cargo run -p void-cli -- --data-dir /tmp/voidnet-b sessions
```

## Current Limits

This slice introduces real runtime-aware coordination and propagation, but it is still early-stage infrastructure.

This document now underpins the secure chat control/data plane described in `docs/operations/secure-peer-communication.md`.

Still not completed in this layer:

- gateway bridging
- richer runtime mount orchestration across apps
- encrypted group payload fanout beyond membership coordination