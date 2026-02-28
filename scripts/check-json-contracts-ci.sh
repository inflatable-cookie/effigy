#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVENT_NAME="${GITHUB_EVENT_NAME:-}"
BASE_REF="${GITHUB_BASE_REF:-}"

if [[ "$EVENT_NAME" == "pull_request" ]]; then
  if [[ -z "$BASE_REF" ]]; then
    echo "[warn] GITHUB_BASE_REF not set; falling back to fast JSON contract checks"
    exec "$ROOT_DIR/scripts/check-json-contracts.sh" --fast --print-selected=json
  fi

  echo "[info] PR mode: resolving changed schema entries vs origin/$BASE_REF"

  if ! git -C "$ROOT_DIR" fetch --no-tags --depth=1 origin "$BASE_REF" >/dev/null 2>&1; then
    echo "[warn] failed to fetch origin/$BASE_REF; falling back to fast JSON contract checks"
    exec "$ROOT_DIR/scripts/check-json-contracts.sh" --fast --print-selected=json
  fi

  BASE_COMMIT="$(git -C "$ROOT_DIR" rev-parse --verify FETCH_HEAD 2>/dev/null || true)"
  if [[ -z "$BASE_COMMIT" ]]; then
    echo "[warn] fetched base ref commit is unavailable; falling back to fast JSON contract checks"
    exec "$ROOT_DIR/scripts/check-json-contracts.sh" --fast --print-selected=json
  fi

  exec "$ROOT_DIR/scripts/check-json-contracts.sh" --fast --changed-only "$BASE_COMMIT" --print-selected=json
fi

echo "[info] non-PR mode: running full JSON contract checks"
exec "$ROOT_DIR/scripts/check-json-contracts.sh" --print-selected=json
