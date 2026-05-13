# Gateway Browser

Purpose: validate gateway visibility and external surface mounting inside VOIDBrowser.

Checks:

- Register and trust a local gateway route.
- Mount a `void://*.gateway.void` route through the browser shell.
- Confirm response preview, bridge sessions, and runtime diagnostics appear in the UI.
- Confirm denied trust or permission states surface as actionable prompts.

Suggested flow:

```sh
cargo run -p void-cli -- gateway register smoke.gateway.void --surface gateway.surface --owner local-node --external-base http://127.0.0.1:18080
cargo run -p void-cli -- gateway allow smoke.gateway.void trusted
cargo run -p void-browser --features desktop-shell
```
