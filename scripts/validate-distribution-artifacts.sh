#!/usr/bin/env bash
set -euo pipefail

ARTIFACTS_DIR=""
EXPECT_HOMEBREW=0

usage() {
  cat <<'USAGE'
Usage: validate-distribution-artifacts.sh --artifacts-dir <dir> [--expect-homebrew]

Validates first-publish artifact logs produced by check-distribution-first-publish.sh.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifacts-dir)
      ARTIFACTS_DIR="$2"
      shift 2
      ;;
    --expect-homebrew)
      EXPECT_HOMEBREW=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$ARTIFACTS_DIR" ]]; then
  echo "[error] --artifacts-dir is required" >&2
  usage >&2
  exit 1
fi

if [[ ! -d "$ARTIFACTS_DIR" ]]; then
  echo "[error] artifacts directory not found: $ARTIFACTS_DIR" >&2
  exit 1
fi

require_log() {
  local label="$1"
  local pattern="$2"

  if compgen -G "$ARTIFACTS_DIR/*${pattern}*.log" > /dev/null; then
    local match
    match="$(find "$ARTIFACTS_DIR" -maxdepth 1 -type f -name "*${pattern}*.log" | sort | head -n1)"
    echo "[ok] $label: $(basename "$match")"
  else
    echo "[error] missing $label log (pattern: *${pattern}*.log)" >&2
    return 1
  fi
}

require_log "tag install validation" "tag-install-validation"
require_log "crates.io install" "crates-io-install-validation"
require_log "crates.io binary help" "crates-io-binary-help"
require_log "crates.io binary json tasks" "crates-io-binary-json-tasks"

if [[ "$EXPECT_HOMEBREW" -eq 1 ]]; then
  require_log "homebrew install" "homebrew-install"
  require_log "homebrew binary help" "homebrew-binary-help"
  require_log "homebrew binary json tasks" "homebrew-binary-json-tasks"
  require_log "homebrew upgrade" "homebrew-upgrade"
fi

echo "[ok] distribution artifact validation passed"
