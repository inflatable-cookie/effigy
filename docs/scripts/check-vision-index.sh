#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
README="$ROOT_DIR/docs/vision/README.md"
VISION_DIR="$ROOT_DIR/docs/vision"
status=0

if [[ ! -f "$README" ]]; then
  echo "[error] vision index not found: ${README#$ROOT_DIR/}" >&2
  exit 1
fi

artifact_lines_file="$(mktemp)"
referenced_paths_file="$(mktemp)"
trap 'rm -f "$artifact_lines_file" "$referenced_paths_file"' EXIT

awk '
  /^## Vision Artifacts$/ { in_section = 1; next }
  /^## / { if (in_section) exit }
  in_section { print }
' "$README" | rg '^\d+\.\s+\[[^]]+\]\(\./[^)]+\)$' > "$artifact_lines_file" || true

if [[ ! -s "$artifact_lines_file" ]]; then
  echo "[error] no vision artifact entries found in docs/vision/README.md" >&2
  exit 1
fi

while IFS= read -r line; do
  rel_path="${line#*](./}"
  rel_path="${rel_path%)}"
  abs_path="$VISION_DIR/$rel_path"
  echo "$rel_path" >> "$referenced_paths_file"

  if [[ ! -f "$abs_path" ]]; then
    echo "[error] missing vision artifact file referenced in README: docs/vision/$rel_path" >&2
    status=1
  fi
done < "$artifact_lines_file"

# Strategy folder policy: keep operational rollout artifacts out of docs/vision.
while IFS= read -r execution_file; do
  [[ -z "$execution_file" ]] && continue
  rel_execution="${execution_file#$VISION_DIR/}"
  echo "[error] execution artifact present in docs/vision/: $rel_execution (move to docs/reports/vision-history/)" >&2
  status=1
done < <(find "$VISION_DIR" -maxdepth 1 -type f \( -name '*-batch-*-closeout-*.md' -o -name '*-compliance-checklist-*.md' \) | sort)

if [[ $status -ne 0 ]]; then
  exit $status
fi

echo "vision index check passed"
