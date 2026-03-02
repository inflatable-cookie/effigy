#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DOCS=true
RUN_JSON=true
CI_MODE=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --docs-only)
      RUN_DOCS=true
      RUN_JSON=false
      shift
      ;;
    --json-only)
      RUN_DOCS=false
      RUN_JSON=true
      shift
      ;;
    --all)
      RUN_DOCS=true
      RUN_JSON=true
      shift
      ;;
    --ci)
      CI_MODE=true
      shift
      ;;
    *)
      echo "[error] unknown option: $1 (supported: --docs-only, --json-only, --all, --ci)" >&2
      exit 1
      ;;
  esac
done

if [[ "$RUN_DOCS" == true ]]; then
  echo "[info] quality gate: docs links"
  docs_files=()
  while IFS= read -r doc_file; do
    docs_files+=("$doc_file")
  done < <(find "$ROOT_DIR/docs" -name '*.md' | sort)
  "$ROOT_DIR/scripts/check-doc-links.sh" "$ROOT_DIR/README.md" "${docs_files[@]}"
  echo "[info] quality gate: docs json examples"
  "$ROOT_DIR/scripts/check-doc-json-examples.sh"
fi

if [[ "$RUN_JSON" == true ]]; then
  echo "[info] quality gate: json contracts"
  if [[ "$CI_MODE" == true ]]; then
    "$ROOT_DIR/scripts/check-json-contracts-ci.sh"
  else
    "$ROOT_DIR/scripts/check-json-contracts.sh" --fast --print-selected=text
  fi
fi

echo "[ok] quality gates passed"
