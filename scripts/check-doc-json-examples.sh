#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLES_FILE="$ROOT_DIR/docs/guides/026-json-payload-examples.md"

if [[ ! -f "$EXAMPLES_FILE" ]]; then
  echo "[error] examples guide not found: $EXAMPLES_FILE" >&2
  exit 1
fi

section_content="$({
  awk '
    /^## 13\) Completion Candidates / {in_section=1}
    in_section {
      if ($0 ~ /^## / && $0 !~ /^## 13\) Completion Candidates /) {
        exit
      }
      print
    }
  ' "$EXAMPLES_FILE"
} )"

if [[ -z "$section_content" ]]; then
  echo "[error] completion candidates section not found in $EXAMPLES_FILE" >&2
  exit 1
fi

json_blocks="$({
  printf '%s\n' "$section_content" | awk '
    /^```json$/ {capture=1; block++; next}
    /^```$/ && capture {capture=0; next}
    capture {
      print "BLOCK:" block ":" $0
    }
  '
} )"

extract_block() {
  local block_no="$1"
  printf '%s\n' "$json_blocks" | sed -n "s/^BLOCK:${block_no}://p"
}

first_block="$(extract_block 1)"
second_block="$(extract_block 2)"

if [[ -z "$first_block" || -z "$second_block" ]]; then
  echo "[error] expected at least two JSON example blocks in completion candidates section" >&2
  exit 1
fi

require_in_block() {
  local block_label="$1"
  local block_content="$2"
  local needle="$3"
  if ! printf '%s\n' "$block_content" | grep -Fq "$needle"; then
    echo "[error] missing '$needle' in $block_label" >&2
    exit 1
  fi
}

required_keys=(
  '"schema": "effigy.completion.candidates.v1"'
  '"schema_version": 1'
  '"cache_state":'
  '"cache_age_ms":'
  '"cache_ttl_ms":'
  '"effective_cache_ttl_ms":'
  '"cache_ttl_source":'
)

for key in "${required_keys[@]}"; do
  require_in_block "completion-candidates example block #1" "$first_block" "$key"
  require_in_block "completion-candidates example block #2" "$second_block" "$key"
done

require_in_block "completion-candidates example block #1" "$first_block" '"cache_state": "hit"'
require_in_block "completion-candidates example block #2" "$second_block" '"cache_hit": false'

echo "examples json check passed"
