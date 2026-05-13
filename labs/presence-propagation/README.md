# Presence Propagation Lab

Purpose: validate distributed room presence and peer activity propagation.

Scenarios:
- Peer join propagation
- Peer leave propagation
- Presence heartbeat synchronization
- Connected peer visibility in runtime bindings

Suggested flow:
1. Start two nodes and let them discover each other on the QUIC mesh.
2. On node A, join `operators`; on node B, join `operators`.
3. Observe `chat/rooms.json` on both nodes for active member updates.
4. Open `void://chat.void` and confirm `chat.room_members` and `chat.connected_peers` update.
5. Stop node B or issue `void-cli chat leave operators`.
6. Confirm node A sees the member transition to offline or removal via room state sync.

Expected signals:
- `[VOIDNET][CHAT] RoomJoined`
- `[VOIDNET][CHAT] RoomLeft`
- `[VOIDNET][CHAT] PresenceUpdated`
- `[VOIDNET][CHAT] RoomStateSynchronized`
