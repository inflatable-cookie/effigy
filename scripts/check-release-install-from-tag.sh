#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_REPO_URL="https://github.com/inflatable-cookie/effigy.git"
REPO_URL="$DEFAULT_REPO_URL"
TAG="${GITHUB_REF_NAME:-}"

run_step() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@"
  echo "[ok] $label"
}

run_step_quiet() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@" >/dev/null
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

if [[ -z "$TAG" ]]; then
  echo "[error] tag is required (pass --tag <tag> or set GITHUB_REF_NAME)" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/effigy-release-install-XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

INSTALL_ROOT="$TMP_DIR/install-root"
FIXTURE_DIR="$TMP_DIR/fixture"

mkdir -p "$FIXTURE_DIR"

cat >"$FIXTURE_DIR/effigy.toml" <<'TOML'
[catalog]
alias = "farmyard"

[tasks]
noop = "echo noop"
TOML

run_step "cargo install from git tag ($TAG)" \
  cargo install --git "$REPO_URL" --tag "$TAG" --root "$INSTALL_ROOT" --force effigy

INSTALLED_BIN="$INSTALL_ROOT/bin/effigy"
if [[ ! -x "$INSTALLED_BIN" ]]; then
  echo "[error] installed binary is missing or not executable: $INSTALLED_BIN" >&2
  exit 1
fi

run_step_quiet "installed binary help" "$INSTALLED_BIN" help
run_step_quiet "installed binary tasks fixture check" "$INSTALLED_BIN" tasks --repo "$FIXTURE_DIR"
run_step_quiet "installed binary prefixed builtin tasks check" "$INSTALLED_BIN" farmyard/tasks --repo "$FIXTURE_DIR"
run_step_quiet "installed binary json help check" "$INSTALLED_BIN" --json help

echo "[ok] release install validation from tag passed"
