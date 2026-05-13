#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# VS Code installed via Snap can inject GTK/glib paths from an older runtime.
# Clear those overrides so Tauri links against the host desktop stack.
unset LD_LIBRARY_PATH
unset GTK_PATH
unset GTK_DATA_PREFIX
unset GTK_EXE_PREFIX
unset GDK_PIXBUF_MODULEDIR
unset GIO_MODULE_DIR

source "$HOME/.cargo/env"
exec cargo run -p void-browser --features desktop-shell "$@"