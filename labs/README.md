# VOIDNET Labs

The labs directory is for distributed systems experiments around the transport substrate. Labs are allowed to be rougher than production code, but they should remain diagnostic, reproducible, and protocol-focused.

Current labs:

- `swarm-simulator`: local multi-node bootstrapping.
- `transport-chaos`: reconnect and failure injection notes.
- `reconnect-tests`: peer restart and bootstrap recovery exercises.
- `latency-tests`: ping and latency observation.
- `runtime-coordination`: runtime-aware mesh visibility and coordination checks.
- `encrypted-sessions`: encrypted session visibility checks.
- `topology-visualiser`: terminal topology rendering.
- `gateway-runtime`: gateway registration, mounting, and topology projection.
- `gateway-routing`: external route abstraction over `void://` gateway paths.
- `gateway-trust`: trust policy enforcement and denial checks.
- `http-bridge`: HTTP bridge metadata and runtime lifecycle checks.
- `http-bridge-runtime`: bridge session lifecycle, latency, snapshots, and diagnostics.
- `response-surfaces`: terminal-safe rendering checks for text, JSON, and HTML fallback.
- `gateway-fetch`: fetch lifecycle, timeout, and failure-path exercises.
- `resource-mounting`: external resource mounting through `void://*.gateway.void`.
- `browser-shell`: thin graphical shell above the runtime.
- `runtime-render-bridge`: `RuntimeSurfaceView` projection into the browser renderer.
- `graphical-runtime`: diagnostics, prompts, sessions, and runtime event bridging in the GUI.
- `gateway-browser`: gateway-backed surface mounting inside VOIDBrowser.

