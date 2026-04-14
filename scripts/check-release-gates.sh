#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.
# - Implementation now delegates to a file-backed Rhai script so the wrapper
#   path exercises the same native scripting surface used by manifest tasks.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

exec effigy __rhai-step \
  --file "$ROOT_DIR/scripts/rhai/check-release-gates.rhai" \
  --repo-root "$ROOT_DIR" \
  --task-name "compat:check-release-gates" \
  -- "$@"
