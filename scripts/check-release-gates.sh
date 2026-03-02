#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG=""
REPO_URL=""

run_step() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@"
  echo "[ok] $label"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"
      shift 2
      ;;
    --repo-url)
      REPO_URL="$2"
      shift 2
      ;;
    *)
      echo "[error] unknown option: $1 (supported: --tag, --repo-url)" >&2
      exit 1
      ;;
  esac
done

cd "$ROOT_DIR"

run_step "format check" cargo fmt --all -- --check
run_step "full test suite" cargo test
run_step "quality gates (ci mode)" ./scripts/check-quality-gates.sh --all --ci
run_step "release binary build" cargo build --release --bin effigy
run_step "release binary smoke" ./scripts/check-release-smoke.sh ./target/release/effigy
if [[ -n "$TAG" ]]; then
  metadata_args=(--tag "$TAG")
  run_step "distribution metadata validation" ./scripts/check-distribution-metadata.sh "${metadata_args[@]}"
else
  run_step "distribution metadata validation" ./scripts/check-distribution-metadata.sh
fi
if [[ -n "$TAG" ]]; then
  install_args=(--tag "$TAG")
  if [[ -n "$REPO_URL" ]]; then
    install_args+=(--repo-url "$REPO_URL")
  fi
  run_step "release install validation from tag" ./scripts/check-release-install-from-tag.sh "${install_args[@]}"
else
  echo "[info] skipping tag install validation (no --tag provided)"
fi

echo "[ok] release gates passed"
