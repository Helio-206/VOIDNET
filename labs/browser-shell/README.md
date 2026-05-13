# Browser Shell

Purpose: validate the thin graphical shell above the existing runtime.

Checks:

- Build the browser backend without desktop bindings.
- Enable the desktop shell feature when Linux GUI dependencies are present.
- Mount `void://chat.void` and confirm runtime-backed navigation.
- Confirm the shell keeps runtime ownership of sessions, permissions, and routing.

Run:

```sh
cargo check -p void-browser --all-targets
cargo check -p void-browser --all-targets --features desktop-shell
```
