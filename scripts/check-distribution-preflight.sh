#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG=""
SKIP_DOCS=0
SKIP_SMOKE=0

usage() {
  cat <<'USAGE'
Usage: check-distribution-preflight.sh [options]

Runs non-publish distribution readiness checks in one command.

Options:
  --tag <vX.Y.Z>     Optional tag for version/tag alignment checks
  --skip-docs        Skip docs link quality gate
  --skip-smoke       Skip distribution artifact pipeline smoke
  --help             Show this help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"
      shift 2
      ;;
    --skip-docs)
      SKIP_DOCS=1
      shift
      ;;
    --skip-smoke)
      SKIP_SMOKE=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

run_step() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@"
  echo "[ok] $label"
}

cd "$ROOT_DIR"

if [[ "$SKIP_DOCS" -eq 0 ]]; then
  run_step "docs link quality gate" ./scripts/check-quality-gates.sh --docs-only
else
  echo "[info] skipping docs link quality gate (--skip-docs)"
fi

metadata_args=()
if [[ -n "$TAG" ]]; then
  metadata_args+=(--tag "$TAG")
fi
run_step "distribution metadata validation" ./scripts/check-distribution-metadata.sh "${metadata_args[@]}"

if [[ "$SKIP_SMOKE" -eq 0 ]]; then
  run_step "distribution artifact pipeline smoke" ./scripts/check-distribution-artifact-pipeline-smoke.sh
else
  echo "[info] skipping distribution artifact pipeline smoke (--skip-smoke)"
fi

echo "[ok] distribution preflight checks passed"
if [[ -n "$TAG" ]]; then
  echo "[next] real publish-cycle command: ./scripts/check-distribution-first-publish.sh --tag $TAG --artifacts-dir ./artifacts/distribution-$TAG"
else
  echo "[next] real publish-cycle command: ./scripts/check-distribution-first-publish.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z"
fi
