# Chat Sync Lab

Purpose: validate live inbox synchronization and room-native message propagation across peers.

Scenarios:
- Multi-peer room message propagation
- Inbox synchronization after delayed delivery
- Notification propagation on new message
- Runtime surface rerender after inbound message

Suggested flow:
1. Start two or three nodes with distinct `--data-dir` values.
2. Join the same room from each node using `void-cli chat join <room>` or `open void://chat.void` and `chat.join`.
3. Send a room message with `void-cli chat room-send <room> <message>` from one node.
4. Confirm on the other nodes:
   - `chat/inbox.json` gains a new room-scoped entry
   - `chat/notifications.json` gains an unread notification
   - `void-cli diagnostics` shows updated `chat_unread_messages`
   - `open void://chat.void` rerenders with updated `chat.inbox_messages`
5. Run `void-cli chat mark-read --room <room>` and confirm unread counts clear.

Expected signals:
- `[VOIDNET][CHAT] MessageReceived`
- `[VOIDNET][CHAT] InboxSynchronized`
- `[VOIDNET][CHAT] NotificationRaised`
- `[VOIDNET][SURFACE] SurfaceUpdated ... chat.inbox_messages`
