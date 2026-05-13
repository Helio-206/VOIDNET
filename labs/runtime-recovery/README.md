# Runtime Recovery Lab

Purpose: validate chat state recovery after restart or runtime reload.

Scenarios:
- Restore current room from persisted state
- Restore inbox and unread counts
- Restore notifications into runtime bindings
- Rehydrate runtime-native chat surface after restart

Suggested flow:
1. Join a room and exchange direct and room messages.
2. Open `void://chat.void` and confirm current room, unread, notifications, and active room history.
3. Stop the node.
4. Restart the node with the same `--data-dir`.
5. Run `void-cli diagnostics` and confirm:
   - `chat_current_room`
   - `chat_unread_messages`
   - `chat_unread_notifications`
   - `chat_room_sync_revision`
6. Open `void://chat.void` again and confirm bindings recover without manual rebuild.

Expected signals:
- `[VOIDNET][CHAT] SessionRecovered`
- `[VOIDNET][CHAT] InboxSynchronized`
- `[VOIDNET][SURFACE] SurfaceLoaded`
- `[VOIDNET][SURFACE] SurfaceStateChanged`
