# VOIDNET

VOIDNET is an experimental decentralized network ecosystem: a private internet-like layer that runs above existing internet transport.

It is not a SaaS app, a CRUD backend, or a blockchain clone. The goal is a compact systems project made of a peer-to-peer node, custom protocol, decentralized identity, name resolution, runtime, browser shell, distributed state layer, event architecture, and first-party applications.

## Status

Phase 1 foundation scaffold:

- Rust workspace for protocol, identity, DNS, transport, runtime, storage, apps, and SDK.
- Go workspace for infrastructure SDK/tooling.
- VOID Protocol v1 draft with typed binary frames.
- VOID DNS architecture for `.void` domains.
- Deterministic ed25519 identity model.
- Event-driven transport/runtime boundaries.
- Typed VOID UI AST and parser.
- System layer model for transport, identity, routing, DNS, runtime, state, application, and browser surface.
- Threat model, trust flow, runtime philosophy, distributed event bus, and formal RFC index.
- Minimal VOIDNode, VOID CLI, VOIDChat, and VOIDBrowser shells.

## Architecture

```text
apps/
  void-browser/   minimal VOID runtime surface
  void-node/      peer node bootstrap shell
  void-chat/      first validation app
  void-cli/       operator tooling

core/
  protocol/       void:// URI, frames, envelopes, stream/app/content messages
  dns/            .void record model and local cache
  identity/       ed25519 node identity, signing, persistence
  transport/      libp2p/QUIC event boundary
  runtime/        app permissions, URI opening, DNS/transport bridge
  storage/        sled-backed namespace store

sdk/
  go/             Go SDK/tooling entry point
  rust/           Rust SDK re-exports
```

## Quick Start

```sh
cargo fmt --all
cargo check --workspace --all-targets
go test ./sdk/go/...
```

Run a local node shell:

```sh
cargo run -p void-node
```

Run the graphical VOIDBrowser shell:

```sh
bash scripts/run-void-browser.sh
```

If your editor terminal comes from Snap-packaged VS Code, use the launcher above instead of raw `cargo run` so Snap-injected GTK/glib paths do not break Tauri at startup.

Run two local nodes:

```sh
cargo run -p void-node -- --data-dir /tmp/voidnet-a --listen /ip4/127.0.0.1/udp/39101/quic-v1
cargo run -p void-node -- --data-dir /tmp/voidnet-b --listen /ip4/127.0.0.1/udp/39102/quic-v1 --bootstrap /ip4/127.0.0.1/udp/39101/quic-v1
```

Inspect protocol parsing:

```sh
cargo run -p void-cli -- uri void://chat.void/rooms/main
cargo run -p void-cli -- domain chat.void
```

Inspect local transport topology:

```sh
cargo run -p void-cli -- peers
cargo run -p void-cli -- topology
cargo run -p void-cli -- diagnostics
cargo run -p void-cli -- runtime
cargo run -p void-cli -- sessions
cargo run -p void-cli -- dns list
cargo run -p void-cli -- dns cache
cargo run -p void-cli -- runtime mounts
cargo run -p void-cli -- runtime sessions
```

Drive encrypted peer communication through the local node command queue:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat peers
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat join operators
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat send 12D3KooW... "hello from node a"
cargo run -p void-cli -- --data-dir /tmp/voidnet-b chat inbox
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-a chat rooms
```

Publish and resolve sovereign `.void` routes through the mesh:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns publish chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-a dns resolve chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-b dns resolve chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-b dns inspect chat.void
```

Open an addressable runtime surface through the local runtime shell:

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-a open void://chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime mounts
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime permissions
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime registry
```

## Design Principles

- Async-first, event-driven components.
- Protocol boundaries before application features.
- Deterministic identity instead of account systems.
- Strongly typed messages instead of ad hoc JSON surfaces.
- Minimal runtime and browser model, not a Chrome clone.
- Local-first storage with a path to replicated state.
- Scoped trust instead of global trust.
- Observable conflict instead of silent overwrite.
- Degraded operation instead of collapse.
- Runtime capabilities instead of ambient authority.

## Documentation

- [Architecture Overview](docs/architecture/overview.md)
- [Architecture Philosophy](docs/architecture/philosophy.md)
- [System Layer Model](docs/architecture/system-layer-model.md)
- [Transport Philosophy](docs/architecture/transport-philosophy.md)
- [Runtime Philosophy](docs/architecture/runtime-philosophy.md)
- [Distributed Event Bus](docs/architecture/distributed-event-bus.md)
- [Distributed State Layer](docs/architecture/distributed-state.md)
- [Trust Flow](docs/architecture/trust-flow.md)
- [Threat Model](docs/architecture/threat-model.md)
- [VOID Protocol v1](docs/protocol/void-protocol-v1.md)
- [VOID UI](docs/protocol/void-ui.md)
- [VOID DNS](docs/architecture/voiddns.md)
- [Identity](docs/architecture/identity.md)
- [Node Lifecycle](docs/architecture/node-lifecycle.md)
- [Phase 1 Roadmap](docs/roadmap/phase-1.md)
- [RFC Index](docs/rfc/README.md)
- [Live Mesh Observability](docs/operations/live-mesh-observability.md)
- [Secure Peer Communication](docs/operations/secure-peer-communication.md)
- [VOIDDNS Routing Operations](docs/operations/voiddns-routing.md)
- [Runtime Shell Operations](docs/operations/runtime-shell.md)

## Repository Posture

This repo is intentionally infrastructure-shaped. The first milestone is not a polished interface; it is a correct foundation for peer discovery, encrypted message transport, deterministic identity, distributed lookup, scoped runtime authority, distributed state, and a runtime that can host VOID applications without inheriting ordinary web platform assumptions.
