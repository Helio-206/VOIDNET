# VOID Terminal Console Smoke Test

## Preconditions

- Workspace root: `/home/helio/HEr/VOID`
- Rust toolchain installed
- Use the same persistent data directory unless you intentionally want a clean node state

## Steps

1. Start a local node:

```bash
cargo run -p void-node
```

2. In another terminal, open the console:

```bash
cargo run -p void-cli -- console
```

3. Confirm the console opens safely:
- Header shows node identity
- Mesh/network state is visible
- Tabs render without layout corruption
- `q` exits back to the shell prompt cleanly

4. Open the runtime chat surface from the command palette:
- Press `:`
- Run:

```text
open chat.void
```

5. Join a room:
- Press `:`
- Run:

```text
join operators
```

6. Switch the active room:
- Press `:`
- Run:

```text
switch operators
```

7. Send a room message:
- Press `:`
- Run either form below:

```text
room-send hello from VOID
```

or

```text
room-send @operators hello from VOID
```

8. Inspect chat state:
- Open the `Chat` tab
- Confirm:
  - room list shows `operators`
  - current room is visible
  - recent room events update
  - inbox/messages render without crashing
  - unread/notifications state is visible

9. Inspect topology:
- Open the `Topology` tab
- Confirm the persisted topology renders without overflow or panic

10. Inspect gateway/runtime state:
- Open the `Dashboard` and `Gateways` tabs
- Confirm mounted route, sessions, permissions, and gateway diagnostics are visible

11. Inspect runtime events:
- Open the `Events` tab
- Confirm recent runtime/node events appear with timestamps and subsystem labels
- Use `j` / `k` or arrow keys to test scrollback

12. Validate command errors do not crash the console:
- Press `:`
- Run:

```text
direct onlypeer
```

- Confirm a usage error is shown and the terminal remains healthy

13. Validate mark-read:
- Press `:`
- Run:

```text
mark-read
```

14. Validate safe exit:
- Press `q`
- Confirm raw mode is restored and the shell prompt returns normally

## Expected Notes

- On a single local node, room publish attempts may report `InsufficientPeers`; this is expected and should appear as readable runtime events rather than a console crash.
- If the terminal is narrow, the console should show a compact fallback view instead of a broken layout.
