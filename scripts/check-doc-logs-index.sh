#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPORTS_DIR="$ROOT_DIR/docs/reports"
REPORTS_INDEX="$REPORTS_DIR/README.md"

if [[ ! -d "$REPORTS_DIR" ]]; then
  echo "[error] reports directory not found: $REPORTS_DIR" >&2
  exit 1
fi

if [[ ! -f "$REPORTS_INDEX" ]]; then
  echo "[error] reports index not found: $REPORTS_INDEX" >&2
  exit 1
fi

all_reports_file="$(mktemp)"
indexed_reports_file="$(mktemp)"
trap 'rm -f "$all_reports_file" "$indexed_reports_file"' EXIT

find "$REPORTS_DIR" -maxdepth 1 -type f -name '*.md' -print \
  | xargs -n1 basename \
  | rg -v '^README\.md$' \
  | sort > "$all_reports_file"

rg -o '\(\./[^)]+\.md\)' "$REPORTS_INDEX" \
  | sed -E 's/^\(\.\///; s/\)$//' \
  | sort -u > "$indexed_reports_file"

missing="$(comm -23 "$all_reports_file" "$indexed_reports_file")"
extra="$(comm -13 "$all_reports_file" "$indexed_reports_file")"

if [[ -n "$missing" ]]; then
  echo "[error] reports index is missing entries:" >&2
  printf '%s\n' "$missing" | sed 's/^/  - /' >&2
  exit 1
fi

if [[ -n "$extra" ]]; then
  echo "[error] reports index references non-existent report files:" >&2
  printf '%s\n' "$extra" | sed 's/^/  - /' >&2
  exit 1
fi

echo "reports index check passed"
