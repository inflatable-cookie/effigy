#!/usr/bin/env bash
set -euo pipefail

# Compatibility wrapper:
# - retained for operators and docs that still invoke the script path directly
# - real logic lives in the Rhai script surface

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EFFIGY_BIN="${EFFIGY_BIN:-${ROOT_DIR}/target/debug/effigy}"

exec "$EFFIGY_BIN" __rhai-step \
  --file "$ROOT_DIR/scripts/rhai/install-local-bin-links.rhai" \
  --repo-root "$ROOT_DIR" \
  --task-name "scripts/install-local-bin-links.sh" \
  -- "$@"
