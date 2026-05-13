# Distributed Rooms Lab

Purpose: validate room snapshot reconciliation, merge behavior, and reconnect safety.

Scenarios:
- Room snapshot merge after late join
- Membership reconciliation after reconnect
- Conflict-safe merge of recent room events
- Recovery from stale membership views

Suggested flow:
1. Start node A and node B.
2. Join `operators` from A and emit several room messages.
3. Start or reconnect node C and join `operators`.
4. Confirm A or B responds with room state sync and C merges members and recent room events.
5. Disconnect one peer temporarily, continue room activity, then reconnect it.
6. Confirm its `chat/rooms.json` and `chat/inbox.json` converge after periodic room sync.

Expected signals:
- `[VOIDNET][CHAT] RoomStateSynchronized`
- `[VOIDNET][CHAT] RoomMembershipChanged`
- `[VOIDNET][CHAT] MessageReceived`
- `chat_last_room_event` advancing in `void-cli diagnostics`
