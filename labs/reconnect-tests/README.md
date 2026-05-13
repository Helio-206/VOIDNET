# Reconnect Tests

Purpose: validate mesh recovery.

Exercises:

- Start Node A and Node B.
- Kill Node B.
- Restart Node B using the same data directory.
- Confirm Node A observes disconnect, reconnect, identity continuity, and active ping.
- Inspect `topology.json` using `voidnet peers`.

Run:

```sh
bash labs/reconnect-tests/restart-peer.sh
```
