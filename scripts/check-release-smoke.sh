#!/usr/bin/env bash
set -euo pipefail

# Compatibility wrapper:
# - retained for CI/release/docs tooling that still calls the script entrypoint
# - real smoke logic lives in the Rhai script surface

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EFFIGY_BIN="${EFFIGY_BIN:-${ROOT_DIR}/target/debug/effigy}"

exec "$EFFIGY_BIN" __rhai-step \
  --file "$ROOT_DIR/scripts/rhai/check-release-smoke.rhai" \
  --repo-root "$ROOT_DIR" \
  --task-name "scripts/check-release-smoke.sh" \
  -- "$@"
