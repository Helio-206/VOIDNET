#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

DATA_DIR="${1:-$HOME/.voidnet/client}"

if [[ -x "$REPO_ROOT/target/debug/void" ]]; then
  CLI_CMD=("$REPO_ROOT/target/debug/void")
else
  CLI_CMD=(cargo run -q -p void-cli --)
fi

run_check() {
  local title="$1"
  shift
  printf '\n=== %s ===\n' "$title"
  "${CLI_CMD[@]}" --data-dir "$DATA_DIR" "$@"
}

echo "[VOIDNET][WAN] smoke-check data_dir=$DATA_DIR"
run_check "Bootstrap" network bootstrap
run_check "Reachability" network reachability
run_check "Relays" network relays
run_check "Sessions" network sessions
run_check "Peers" network peers
run_check "Diagnostics" network diagnostics

cat <<'EOF'

[VOIDNET][WAN] What to look for
- bootstrap_connected > 0 means bootstrap connectivity is live
- reachability shows AutoNAT-derived public/private state
- relay_reservations shows accepted relay reservations
- relay_connections shows active relayed peer paths
- hole_punch_attempts and hole_punch_successes show DCUtR progress
- last_error fields explain the most recent degraded path
EOF