#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.

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
    -h|--help)
      cargo run --bin effigy -- release --help
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1 (supported: --tag, --repo-url)" >&2
      exit 1
      ;;
  esac
done

cd "$ROOT_DIR"

run_step "release gates" \
  cargo run --bin effigy -- release gates --repo "$ROOT_DIR"

if [[ -n "$TAG" ]]; then
  verify_args=(cargo run --bin effigy -- release verify-install --repo "$ROOT_DIR" --tag "$TAG")
  if [[ -n "$REPO_URL" ]]; then
    verify_args+=(--repo-url "$REPO_URL")
  fi
  run_step "release install validation from tag" "${verify_args[@]}"
else
  echo "[info] skipping tag install validation (no --tag provided)"
fi

echo "[ok] release gates passed"
