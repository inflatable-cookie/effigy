#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
README="${VISION_README_PATH:-$ROOT_DIR/docs/vision/README.md}"
VISION_DIR="${VISION_DIR_PATH:-$ROOT_DIR/docs/vision}"
VERBS_FILE="${VISION_NEXT_TASK_VERBS_FILE:-$ROOT_DIR/docs/scripts/fixtures/vision-next-task/actionable-verbs.txt}"
status=0

if [[ ! -f "$README" ]]; then
  echo "[error] vision index not found: $README" >&2
  exit 1
fi

if [[ ! -f "$VERBS_FILE" ]]; then
  echo "[error] actionable verb allowlist not found: $VERBS_FILE" >&2
  exit 1
fi

artifact_lines_file="$(mktemp)"
verbs_file_normalized="$(mktemp)"
trap 'rm -f "$artifact_lines_file" "$verbs_file_normalized"' EXIT

awk '
  /^## Vision Artifacts$/ { in_section = 1; next }
  /^## / { if (in_section) exit }
  in_section { print }
' "$README" | rg '^\d+\.\s+\[[^]]+\]\(\./[^)]+\)$' > "$artifact_lines_file" || true

if [[ ! -s "$artifact_lines_file" ]]; then
  echo "[error] no vision artifact entries found in $README" >&2
  exit 1
fi

while IFS= read -r verb_line; do
  verb="$(tr '[:upper:]' '[:lower:]' <<<"$verb_line" | sed -E 's/[[:space:]]+#.*$//; s/^[[:space:]]+|[[:space:]]+$//g')"
  [[ -z "$verb" ]] && continue
  echo "$verb" >> "$verbs_file_normalized"
done < "$VERBS_FILE"

if [[ ! -s "$verbs_file_normalized" ]]; then
  echo "[error] actionable verb allowlist is empty: $VERBS_FILE" >&2
  exit 1
fi

while IFS= read -r line; do
  rel_path="${line#*](./}"
  rel_path="${rel_path%)}"
  file="$VISION_DIR/$rel_path"

  if [[ ! -f "$file" ]]; then
    # existence is handled by check-vision-index, but keep this script self-contained.
    echo "[error] missing vision artifact file: $file" >&2
    status=1
    continue
  fi

  if ! rg -q '^## Next Task$' "$file"; then
    echo "[error] missing '## Next Task' section in docs/vision/$rel_path" >&2
    status=1
    continue
  fi

  first_line="$(
    awk '
      /^## Next Task$/ { sec = 1; next }
      /^## / { if (sec) exit }
      sec {
        line = $0
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
        if (line != "") {
          print line
          exit
        }
      }
    ' "$file"
  )"

  if [[ -z "$first_line" ]]; then
    echo "[error] empty '## Next Task' section in docs/vision/$rel_path" >&2
    status=1
    continue
  fi

  normalized_line="$(sed -E 's/^[[:space:]]*([-*+]|\([0-9]+\)|[0-9]+\.)[[:space:]]+//; s/^[[:space:]]+//' <<<"$first_line")"
  verb="$(awk '{ print $1 }' <<<"$normalized_line" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z].*$//')"

  if [[ -z "$verb" ]] || ! rg -Fxq "$verb" "$verbs_file_normalized"; then
    echo "[error] non-actionable '## Next Task' lead verb in docs/vision/$rel_path: '$first_line' (allowlist: $VERBS_FILE)" >&2
    status=1
  fi
done < "$artifact_lines_file"

if [[ $status -ne 0 ]]; then
  exit $status
fi

echo "vision next-task check passed"
