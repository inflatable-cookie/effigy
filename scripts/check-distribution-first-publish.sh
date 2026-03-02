#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${GITHUB_REF_NAME:-}"
CRATE_VERSION=""
REPO_URL="https://github.com/inflatable-cookie/effigy.git"
BREW_FORMULA="inflatable-cookie/effigy/effigy"
SKIP_HOMEBREW=0

run_step() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@"
  echo "[ok] $label"
}

usage() {
  cat <<'EOF'
Usage:
  ./scripts/check-distribution-first-publish.sh --tag <vX.Y.Z> [options]

Options:
  --tag <tag>              Release tag (required unless GITHUB_REF_NAME is set)
  --crate-version <ver>    crates.io version (defaults to tag without leading v)
  --repo-url <url>         Git repo URL for tag install validation
  --brew-formula <name>    Homebrew formula reference (default: inflatable-cookie/effigy/effigy)
  --skip-homebrew          Skip Homebrew install/upgrade checks
  --help                   Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"
      shift 2
      ;;
    --crate-version)
      CRATE_VERSION="$2"
      shift 2
      ;;
    --repo-url)
      REPO_URL="$2"
      shift 2
      ;;
    --brew-formula)
      BREW_FORMULA="$2"
      shift 2
      ;;
    --skip-homebrew)
      SKIP_HOMEBREW=1
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

if [[ -z "$TAG" ]]; then
  echo "[error] --tag is required (or set GITHUB_REF_NAME)" >&2
  exit 1
fi

if [[ -z "$CRATE_VERSION" ]]; then
  CRATE_VERSION="${TAG#v}"
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/effigy-distribution-first-publish-XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

run_step "tag install validation" \
  "$ROOT_DIR/scripts/check-release-install-from-tag.sh" --tag "$TAG" --repo-url "$REPO_URL"

CRATE_INSTALL_ROOT="$TMP_DIR/crates-install-root"
run_step "crates.io install validation ($CRATE_VERSION)" \
  cargo install effigy --version "$CRATE_VERSION" --locked --root "$CRATE_INSTALL_ROOT" --force

CRATE_BIN="$CRATE_INSTALL_ROOT/bin/effigy"
if [[ ! -x "$CRATE_BIN" ]]; then
  echo "[error] expected crates.io-installed binary at $CRATE_BIN" >&2
  exit 1
fi

run_step "crates.io binary help" "$CRATE_BIN" --help >/dev/null
run_step "crates.io binary json tasks" "$CRATE_BIN" --json tasks >/dev/null

if [[ "$SKIP_HOMEBREW" -eq 1 ]]; then
  echo "[info] skipping homebrew checks (--skip-homebrew)"
elif ! command -v brew >/dev/null 2>&1; then
  echo "[info] skipping homebrew checks (brew not available)"
else
  run_step "homebrew install" brew install "$BREW_FORMULA"
  run_step "homebrew binary help" effigy --help >/dev/null
  run_step "homebrew binary json tasks" effigy --json tasks >/dev/null
  run_step "homebrew upgrade" brew upgrade effigy
fi

echo "[ok] distribution first-publish matrix passed"
