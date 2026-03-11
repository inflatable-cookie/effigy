#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_REPO_URL="https://github.com/inflatable-cookie/effigy.git"
REPO_URL="$DEFAULT_REPO_URL"
TAG="${GITHUB_REF_NAME:-}"

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
      cargo run --bin effigy -- release verify-install --help
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1 (supported: --tag, --repo-url)" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$TAG" ]]; then
  echo "[error] tag is required (pass --tag <tag> or set GITHUB_REF_NAME)" >&2
  exit 1
fi

exec cargo run --bin effigy -- release verify-install \
  --repo "$ROOT_DIR" \
  --tag "$TAG" \
  --repo-url "$REPO_URL"
