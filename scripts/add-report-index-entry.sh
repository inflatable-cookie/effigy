#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORTS_DIR="$ROOT_DIR/docs/reports"
INDEX_FILE="$REPORTS_DIR/README.md"

usage() {
  cat >&2 <<USAGE
usage: $(basename "$0") <report-file>

Examples:
  $(basename "$0") 2026-03-02-my-report.md
  $(basename "$0") docs/reports/2026-03-02-my-report.md
USAGE
}

if [[ $# -ne 1 ]]; then
  usage
  exit 1
fi

input="$1"
base_name="$(basename "$input")"
report_path="$REPORTS_DIR/$base_name"

if [[ "$base_name" == "README.md" ]]; then
  echo "[error] README.md is not a report artifact" >&2
  exit 1
fi

if [[ "${base_name##*.}" != "md" ]]; then
  echo "[error] report must be a .md file: $base_name" >&2
  exit 1
fi

if [[ ! -f "$report_path" ]]; then
  echo "[error] report file not found: $report_path" >&2
  exit 1
fi

if [[ ! -f "$INDEX_FILE" ]]; then
  echo "[error] reports index not found: $INDEX_FILE" >&2
  exit 1
fi

entry="- [\`$base_name\`](./$base_name)"

if rg -Fq -- "$entry" "$INDEX_FILE"; then
  echo "report already indexed: $base_name"
  exit 0
fi

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT

awk -v entry="$entry" '
  BEGIN { inserted = 0 }
  /^## Archived Validation Reports$/ && inserted == 0 {
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

echo "indexed report: $base_name"
