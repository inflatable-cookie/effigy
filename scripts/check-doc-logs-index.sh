#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOGS_DIR="$ROOT_DIR/docs/logs"
LOGS_INDEX="$LOGS_DIR/README.md"

if [[ ! -d "$LOGS_DIR" ]]; then
  echo "[error] logs directory not found: $LOGS_DIR" >&2
  exit 1
fi

if [[ ! -f "$LOGS_INDEX" ]]; then
  echo "[error] logs index not found: $LOGS_INDEX" >&2
  exit 1
fi

all_logs_file="$(mktemp)"
indexed_logs_file="$(mktemp)"
trap 'rm -f "$all_logs_file" "$indexed_logs_file"' EXIT

find "$LOGS_DIR" -mindepth 2 -maxdepth 2 -type f -name '*.md' -print \
  | sed "s#^$LOGS_DIR/##" \
  | grep -v '^README\.md$' \
  | sort > "$all_logs_file"

grep -oE '\(\./[^)]+\.md\)' "$LOGS_INDEX" \
  | sed -E 's/^\(\.\///; s/\)$//' \
  | sort -u > "$indexed_logs_file"

missing="$(comm -23 "$all_logs_file" "$indexed_logs_file")"
extra="$(comm -13 "$all_logs_file" "$indexed_logs_file")"

if [[ -n "$missing" ]]; then
  echo "[error] logs index is missing entries:" >&2
  printf '%s\n' "$missing" | sed 's/^/  - /' >&2
  exit 1
fi

if [[ -n "$extra" ]]; then
  echo "[error] logs index references non-existent log files:" >&2
  printf '%s\n' "$extra" | sed 's/^/  - /' >&2
  exit 1
fi

echo "logs index check passed"
