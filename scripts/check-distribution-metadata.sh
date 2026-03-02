#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG=""

usage() {
  cat <<'USAGE'
Usage: check-distribution-metadata.sh [--tag vX.Y.Z]

Validates release distribution metadata prerequisites:
- Cargo package metadata shape
- release/install documentation presence
- Homebrew/release workflow wiring
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "[error] unknown option: $1 (supported: --tag)" >&2
      exit 1
      ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  echo "[error] jq is required" >&2
  exit 1
fi

run_check() {
  local label="$1"
  shift
  echo "[check] $label"
  "$@"
  echo "[ok] $label"
}

assert_file_exists() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "[error] required file is missing: $path" >&2
    exit 1
  fi
}

validate_package_metadata() {
  local metadata_json pkg_json name version license description

  metadata_json="$(cargo metadata --no-deps --format-version 1)"
  pkg_json="$(jq -c '.packages[] | select(.name == "effigy")' <<<"$metadata_json")"

  if [[ -z "$pkg_json" ]]; then
    echo "[error] package 'effigy' not found in cargo metadata" >&2
    exit 1
  fi

  name="$(jq -r '.name // empty' <<<"$pkg_json")"
  version="$(jq -r '.version // empty' <<<"$pkg_json")"
  license="$(jq -r '.license // empty' <<<"$pkg_json")"
  description="$(jq -r '.description // empty' <<<"$pkg_json")"

  if [[ "$name" != "effigy" ]]; then
    echo "[error] expected package name 'effigy', got '$name'" >&2
    exit 1
  fi

  if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]]; then
    echo "[error] package version is not semver-like: '$version'" >&2
    exit 1
  fi

  if [[ -z "$license" ]]; then
    echo "[error] package license is empty" >&2
    exit 1
  fi

  if [[ -z "$description" ]]; then
    echo "[error] package description is empty" >&2
    exit 1
  fi

  if [[ -n "$TAG" ]]; then
    if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]]; then
      echo "[error] tag must match vX.Y.Z format: '$TAG'" >&2
      exit 1
    fi

    local tag_version="${TAG#v}"
    if [[ "$tag_version" != "$version" ]]; then
      echo "[error] tag version '$tag_version' does not match Cargo version '$version'" >&2
      exit 1
    fi
  fi
}

validate_release_docs_presence() {
  assert_file_exists "$ROOT_DIR/docs/guides/010-path-installation-and-release.md"
  assert_file_exists "$ROOT_DIR/docs/guides/014-release-checklist-template.md"
  assert_file_exists "$ROOT_DIR/docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md"
  assert_file_exists "$ROOT_DIR/docs/guides/042-homebrew-tap-and-release-automation.md"
  assert_file_exists "$ROOT_DIR/docs/guides/044-distribution-first-publish-execution-runbook.md"
}

validate_release_workflow_wiring() {
  assert_file_exists "$ROOT_DIR/.github/workflows/release-gates.yml"
  assert_file_exists "$ROOT_DIR/.github/workflows/homebrew-tap-metadata.yml"
  assert_file_exists "$ROOT_DIR/.github/workflows/homebrew-tap-formula-pr.yml"
  assert_file_exists "$ROOT_DIR/scripts/check-release-install-from-tag.sh"
  assert_file_exists "$ROOT_DIR/scripts/check-distribution-first-publish.sh"
  assert_file_exists "$ROOT_DIR/scripts/update-homebrew-formula-from-metadata.sh"
}

cd "$ROOT_DIR"
run_check "cargo package metadata" validate_package_metadata
run_check "release/install docs presence" validate_release_docs_presence
run_check "release workflow wiring" validate_release_workflow_wiring

echo "[ok] distribution metadata checks passed"
