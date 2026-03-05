#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOGS_DIR="$ROOT_DIR/docs/logs"
INDEX_FILE="$LOGS_DIR/README.md"

usage() {
  cat >&2 <<USAGE
usage: $(basename "$0") <log-file>

Examples:
  $(basename "$0") 2026-03/02-160000-my-log.md
  $(basename "$0") docs/logs/2026-03/02-160000-my-log.md
USAGE
}

if [[ $# -ne 1 ]]; then
  usage
  exit 1
fi

input="$1"
input="${input#./}"
relative_path="$input"

if [[ "$relative_path" == docs/logs/* ]]; then
  relative_path="${relative_path#docs/logs/}"
fi

log_path="$LOGS_DIR/$relative_path"

if [[ "$relative_path" == "README.md" ]]; then
  echo "[error] README.md is not a log artifact" >&2
  exit 1
fi

if [[ "${relative_path##*.}" != "md" ]]; then
  echo "[error] log must be a .md file: $relative_path" >&2
  exit 1
fi

if [[ ! "$relative_path" =~ ^[0-9]{4}-[0-9]{2}/[0-9]{2}-[0-9]{6}-.+\.md$ ]]; then
  echo "[error] log path must match YYYY-MM/DD-HHMMSS-slug.md: $relative_path" >&2
  exit 1
fi

if [[ ! -f "$log_path" ]]; then
  echo "[error] log file not found: $log_path" >&2
  exit 1
fi

if [[ ! -f "$INDEX_FILE" ]]; then
  echo "[error] logs index not found: $INDEX_FILE" >&2
  exit 1
fi

entry="- [\`$relative_path\`](./$relative_path)"

if rg -Fq -- "$entry" "$INDEX_FILE"; then
  echo "log already indexed: $relative_path"
  exit 0
fi

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

awk -v entry="$entry" '
  BEGIN { inserted = 0 }
  /^## Archived Validation Logs$/ && inserted == 0 {
    print entry
    print ""
    inserted = 1
  }
  { print }
  END {
    if (inserted == 0) {
      print ""
      print entry
    }
  }
' "$INDEX_FILE" > "$tmp_file"

mv "$tmp_file" "$INDEX_FILE"

echo "indexed log: $relative_path"
