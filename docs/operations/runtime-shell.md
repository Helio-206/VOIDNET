# Runtime Shell Operations

Scope: executable runtime surfaces mounted through `void://` on top of the existing addressable mesh.

This slice adds a persistent local runtime shell under `data_dir/runtime/shell.json`.

The shell now tracks:

- runtime surface registry
- mounted surfaces
- runtime sessions
- permission grants and denials
- mount failures

## Mount Flow

`void-cli open void://chat.void` now performs this flow:

1. resolve the `.void` authority through VOIDDNS
2. resolve the target peer and runtime surface
3. request capabilities for the route
4. auto-grant safe runtime capabilities or reject sensitive ones without an explicit grant
5. create a runtime session record
6. persist the mounted surface and session state
7. emit runtime lifecycle events

Current lifecycle states:

- `UNRESOLVED`
- `RESOLVING`
- `NEGOTIATING`
- `MOUNTING`
- `ACTIVE`
- `SUSPENDED`
- `FAILED`
- `UNMOUNTED`

## Commands

```sh
cargo run -p void-cli -- --data-dir /tmp/voidnet-a open void://chat.void
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime mounts
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime sessions
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime permissions
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime permissions grant vault 12D3Koo... storage
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime permissions deny vault 12D3Koo... filesystem
cargo run -p void-cli -- --data-dir /tmp/voidnet-a runtime registry
```

## Registry

The runtime shell bootstraps a local registry entry for `chat.void` backed by the `void-chat` surface. The registry view also checks whether each registered surface is currently published in VOIDDNS.

## Event Surface

The runtime shell emits and prints:

- `SurfaceResolving`
- `SurfaceResolved`
- `SurfaceMounting`
- `SurfaceMounted`
- `SurfaceUnmounted`
- `CapabilityRequested`
- `CapabilityGranted`
- `CapabilityRejected`
- `RuntimeSessionStarted`
- `RuntimeSessionClosed`

## Diagnostics

Local topology diagnostics now include runtime-shell state:

- mounted surface count
- active session count
- active permission count
- failed mount count
- registry entry count
- last mount latency

This state is persisted back into `topology.json` so the existing `diagnostics` and `topology` CLI views expose runtime-shell activity.