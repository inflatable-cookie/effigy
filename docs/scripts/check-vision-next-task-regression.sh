#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="$ROOT_DIR/docs/scripts/check-vision-next-task.sh"
FIXTURE_BASE="$ROOT_DIR/docs/scripts/fixtures/vision-next-task/base"
VERBS_FILE="$ROOT_DIR/docs/scripts/fixtures/vision-next-task/actionable-verbs.txt"

if [[ ! -x "$CHECKER" ]]; then
  echo "[error] checker not executable: $CHECKER" >&2
  exit 1
fi

if [[ ! -d "$FIXTURE_BASE" ]]; then
  echo "[error] fixture base not found: $FIXTURE_BASE" >&2
  exit 1
fi

if [[ ! -f "$VERBS_FILE" ]]; then
  echo "[error] verb allowlist fixture not found: $VERBS_FILE" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

setup_case() {
  local case_name="$1"
  local case_dir="$tmp_dir/$case_name"
  cp -R "$FIXTURE_BASE" "$case_dir"
  echo "$case_dir"
}

run_checker() {
  local case_dir="$1"
  VISION_README_PATH="$case_dir/README.md" \
  VISION_DIR_PATH="$case_dir" \
  VISION_NEXT_TASK_VERBS_FILE="$VERBS_FILE" \
  "$CHECKER" >/dev/null 2>&1
}

expect_pass() {
  local case_name="$1"
  local case_dir
  case_dir="$(setup_case "$case_name")"

  if ! run_checker "$case_dir"; then
    echo "[error] expected pass but failed: $case_name" >&2
    return 1
  fi

  return 0
}

expect_fail() {
  local case_name="$1"
  local case_dir
  case_dir="$(setup_case "$case_name")"

  case "$case_name" in
    missing-next-task-heading)
      perl -0pi -e 's/## Next Task/## Next Steps/' "$case_dir/001-blueprint.md"
      ;;
    empty-next-task)
      cat > "$case_dir/002-checklist.md" <<'CASE'
# Fixture Checklist

## Next Task

## Notes

placeholder
CASE
      ;;
    non-actionable-verb)
      cat > "$case_dir/005-batch-2-closeout-2026-03-05.md" <<'CASE'
# Fixture Closeout

## Next Task

Consider future cleanup scheduling.
CASE
      ;;
    *)
      echo "[error] unknown failure fixture case: $case_name" >&2
      return 1
      ;;
  esac

  if run_checker "$case_dir"; then
    echo "[error] expected failure but passed: $case_name" >&2
    return 1
  fi

  return 0
}

expect_pass baseline
expect_fail missing-next-task-heading
expect_fail empty-next-task
expect_fail non-actionable-verb
expect_pass actionable-bullet-prefix

echo "vision next-task regression check passed"
