# Transport Chaos

Purpose: inject hostile transport conditions once the local swarm is compiling and running.

Initial scenarios:

- Kill a connected peer and observe `PeerDisconnected`.
- Restart a peer with the same identity and observe reconnect.
- Start a peer with a fresh identity on an old address and observe authentication behavior.
- Flood bootstrap attempts and verify low-noise `TransportFailed` output.

Bootstrap recovery:

```sh
bash labs/transport-chaos/bootstrap-recovery.sh
```
