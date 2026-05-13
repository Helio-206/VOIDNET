# Graphical Runtime

Purpose: inspect runtime diagnostics, sessions, and event flow through the graphical shell.

Checks:

- Confirm the diagnostics panel reports mounts, peers, permissions, and bridge sessions.
- Confirm the event bridge panel mirrors runtime and gateway lifecycle lines.
- Confirm pending permission prompts appear when a surface is denied capability access.
- Confirm resolving a prompt re-enters the normal runtime navigation path.

Run:

```sh
cargo check -p void-browser --all-targets
cargo run -p void-browser --features desktop-shell
```
