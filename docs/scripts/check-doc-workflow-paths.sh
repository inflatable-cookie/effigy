#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
pattern='\.github(-bak)?/workflows/[A-Za-z0-9._-]+\.ya?ml'
status=0
matches=0

while IFS= read -r hit; do
  [[ -z "$hit" ]] && continue
  matches=1

  file="${hit%%:*}"
  rest="${hit#*:}"
  line="${rest%%:*}"
  workflow_path="${rest#*:}"

  candidate="$ROOT_DIR/$workflow_path"
  if [[ -f "$candidate" ]]; then
    continue
  fi

  if [[ "$workflow_path" == .github/workflows/* ]]; then
    alt=".github-bak/workflows/${workflow_path##*/}"
    if [[ -f "$ROOT_DIR/$alt" ]]; then
      echo "[error] stale workflow path in ${file#$ROOT_DIR/}:${line}: $workflow_path (use $alt)" >&2
      status=1
      continue
    fi
  fi

  if [[ "$workflow_path" == .github-bak/workflows/* ]]; then
    alt=".github/workflows/${workflow_path##*/}"
    if [[ -f "$ROOT_DIR/$alt" ]]; then
      echo "[error] stale workflow path in ${file#$ROOT_DIR/}:${line}: $workflow_path (use $alt)" >&2
      status=1
      continue
    fi
  fi

  echo "[error] missing workflow path in ${file#$ROOT_DIR/}:${line}: $workflow_path" >&2
  status=1
done < <(find "$ROOT_DIR/docs" -name '*.md' ! -path "$ROOT_DIR/docs/reports/*" -print0 | xargs -0 rg -n -o "$pattern" || true)

if [[ $matches -eq 0 ]]; then
  echo "[warn] no workflow references found in non-report docs"
fi

if [[ $status -ne 0 ]]; then
  exit $status
fi

echo "doc workflow path check passed"
