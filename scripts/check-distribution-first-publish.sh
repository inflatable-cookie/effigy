#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${GITHUB_REF_NAME:-}"
CRATE_VERSION=""
REPO_URL="https://github.com/inflatable-cookie/effigy.git"
BREW_FORMULA="inflatable-cookie/effigy/effigy"
SKIP_HOMEBREW=0
ARTIFACTS_DIR=""
CLEANUP_DIR=""
STEP_INDEX=0
HOME_BREW_EXECUTED=0
LOG_FILES=()

slugify() {
  tr '[:upper:]' '[:lower:]' <<<"$1" | sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//; s/-+/-/g'
}

run_step() {
  local label="$1"
  shift

  STEP_INDEX=$((STEP_INDEX + 1))
  local slug
  slug="$(slugify "$label")"
  local log_path
  log_path="$ARTIFACTS_DIR/$(printf '%02d' "$STEP_INDEX")-${slug}.log"
  LOG_FILES+=("$(basename "$log_path")")

  echo "[check] $label"
  if "$@" >"$log_path" 2>&1; then
    echo "[ok] $label (log: $log_path)"
  else
    echo "[error] $label failed (log: $log_path)" >&2
    echo "[error] tail of log:" >&2
    tail -n 40 "$log_path" >&2 || true
    exit 1
  fi
}

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/check-distribution-first-publish.sh --tag <vX.Y.Z> [options]

Options:
  --tag <tag>              Release tag (required unless GITHUB_REF_NAME is set)
  --crate-version <ver>    crates.io version (defaults to tag without leading v)
  --repo-url <url>         Git repo URL for tag install validation
  --brew-formula <name>    Homebrew formula reference (default: inflatable-cookie/effigy/effigy)
  --skip-homebrew          Skip Homebrew install/upgrade checks
  --artifacts-dir <dir>    Preserve step logs in a specific directory
  --help                   Show this help
USAGE
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
    --artifacts-dir)
      ARTIFACTS_DIR="$2"
      shift 2
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

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]]; then
  echo "[error] tag must match vX.Y.Z format: $TAG" >&2
  exit 1
fi

if [[ -z "$CRATE_VERSION" ]]; then
  CRATE_VERSION="${TAG#v}"
fi

if [[ -z "$ARTIFACTS_DIR" ]]; then
  CLEANUP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/effigy-distribution-first-publish-XXXXXX")"
  ARTIFACTS_DIR="$CLEANUP_DIR"
else
  mkdir -p "$ARTIFACTS_DIR"
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/effigy-distribution-first-publish-work-XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
  if [[ -n "$CLEANUP_DIR" ]]; then
    rm -rf "$CLEANUP_DIR"
  fi
}
trap cleanup EXIT

run_step "tag install validation" \
  "$ROOT_DIR/scripts/check-release-install-from-tag.sh" --tag "$TAG" --repo-url "$REPO_URL"

CRATE_INSTALL_ROOT="$TMP_DIR/crates-install-root"
run_step "crates.io install validation (${CRATE_VERSION})" \
  cargo install effigy --version "$CRATE_VERSION" --locked --root "$CRATE_INSTALL_ROOT" --force

CRATE_BIN="$CRATE_INSTALL_ROOT/bin/effigy"
if [[ ! -x "$CRATE_BIN" ]]; then
  echo "[error] expected crates.io-installed binary at $CRATE_BIN" >&2
  exit 1
fi

run_step "crates.io binary help" "$CRATE_BIN" --help
run_step "crates.io binary json tasks" "$CRATE_BIN" --json tasks

if [[ "$SKIP_HOMEBREW" -eq 1 ]]; then
  echo "[info] skipping homebrew checks (--skip-homebrew)"
elif ! command -v brew >/dev/null 2>&1; then
  echo "[info] skipping homebrew checks (brew not available)"
else
  HOME_BREW_EXECUTED=1
  run_step "homebrew install" brew install "$BREW_FORMULA"
  run_step "homebrew binary help" effigy --help
  run_step "homebrew binary json tasks" effigy --json tasks
  run_step "homebrew upgrade" brew upgrade effigy
fi

summary_path="$ARTIFACTS_DIR/distribution-summary.env"
{
  echo "TAG=$TAG"
  echo "CRATE_VERSION=$CRATE_VERSION"
  echo "REPO_URL=$REPO_URL"
  echo "BREW_FORMULA=$BREW_FORMULA"
  echo "HOMEBREW_EXECUTED=$HOME_BREW_EXECUTED"
  echo "LOG_FILES=$(IFS=,; echo "${LOG_FILES[*]}")"
} > "$summary_path"

validate_args=(--artifacts-dir "$ARTIFACTS_DIR")
if [[ "$HOME_BREW_EXECUTED" -eq 1 ]]; then
  validate_args+=(--expect-homebrew)
fi
"$ROOT_DIR/scripts/validate-distribution-artifacts.sh" "${validate_args[@]}"

echo "[ok] distribution first-publish matrix passed"
echo "[ok] artifacts directory: $ARTIFACTS_DIR"
echo "[ok] artifacts summary: $summary_path"
