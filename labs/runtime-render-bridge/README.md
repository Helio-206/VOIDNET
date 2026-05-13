# Runtime Render Bridge

Purpose: verify that VOIDBrowser renders `RuntimeSurfaceView` trees instead of duplicating runtime logic.

Checks:

- Open a runtime-backed route such as `void://chat.void`.
- Confirm the mounted surface tree projects into the DOM renderer.
- Dispatch a surface action and verify the runtime updates bindings and input state.
- Trigger `browser_sync` and confirm the shell rehydrates from runtime state.

Run:

```sh
cargo check -p void-browser --all-targets
cargo test -p void-runtime --lib
```
