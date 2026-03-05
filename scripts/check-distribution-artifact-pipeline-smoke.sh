#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/effigy-distribution-artifact-pipeline-smoke-XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

ARTIFACTS_DIR="$TMP_DIR/artifacts"
REPORT_PATH="$TMP_DIR/closeout-report.md"
mkdir -p "$ARTIFACTS_DIR"

for name in \
  01-tag-install-validation.log \
  02-crates-io-install-validation-0-1-0.log \
  03-crates-io-binary-help.log \
  04-crates-io-binary-json-tasks.log \
  05-homebrew-install.log \
  06-homebrew-binary-help.log \
  07-homebrew-binary-json-tasks.log \
  08-homebrew-upgrade.log
  do
  touch "$ARTIFACTS_DIR/$name"
done

cat > "$ARTIFACTS_DIR/distribution-summary.env" <<'SUMMARY'
TAG=v0.1.0
CRATE_VERSION=0.1.0
REPO_URL=https://github.com/inflatable-cookie/effigy.git
BREW_FORMULA=inflatable-cookie/effigy/effigy
HOMEBREW_EXECUTED=1
LOG_FILES=01-tag-install-validation.log,02-crates-io-install-validation-0-1-0.log,03-crates-io-binary-help.log,04-crates-io-binary-json-tasks.log,05-homebrew-install.log,06-homebrew-binary-help.log,07-homebrew-binary-json-tasks.log,08-homebrew-upgrade.log
SUMMARY

echo "[check] validate artifact bundle"
"$ROOT_DIR/scripts/validate-distribution-artifacts.sh" --artifacts-dir "$ARTIFACTS_DIR" --expect-homebrew

echo "[check] generate closeout report"
"$ROOT_DIR/scripts/generate-distribution-closeout-report.sh" \
  --tag v0.1.0 \
  --artifacts-dir "$ARTIFACTS_DIR" \
  --output "$REPORT_PATH"

echo "[check] assert report content"
grep -q '^# Distribution Acceptance Closeout (v0.1.0)$' "$REPORT_PATH"
grep -q '^- Homebrew evidence included: true\.$' "$REPORT_PATH"
grep -q '^- 08-homebrew-upgrade.log$' "$REPORT_PATH"

echo "[ok] distribution artifact pipeline smoke passed"
