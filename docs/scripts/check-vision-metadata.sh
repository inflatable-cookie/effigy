#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

require_heading() {
  local file="$1"
  local heading="$2"
  if ! rg -q "^${heading}$" "$file"; then
    echo "[error] missing heading '${heading}' in ${file#$ROOT_DIR/}" >&2
    return 1
  fi
}

status=0

roadmap_files=(
  "$ROOT_DIR/docs/roadmap/001-effigy-foundation.md"
  "$ROOT_DIR/docs/roadmap/002-deferral-fallback-system.md"
  "$ROOT_DIR/docs/roadmap/003-coloured-output-and-widgets.md"
  "$ROOT_DIR/docs/roadmap/004-dev-process-manager-tui.md"
  "$ROOT_DIR/docs/roadmap/005-unified-testing-orchestration.md"
  "$ROOT_DIR/docs/roadmap/006-tui-terminal-emulation.md"
  "$ROOT_DIR/docs/roadmap/007-test-runner-selection-and-environment-policy.md"
  "$ROOT_DIR/docs/roadmap/008-universal-json-command-coverage.md"
  "$ROOT_DIR/docs/roadmap/009-doctor-health-consolidation.md"
  "$ROOT_DIR/docs/roadmap/010-dag-lock-policy-baseline.md"
  "$ROOT_DIR/docs/roadmap/011-watch-init-migrate-phase-1.md"
  "$ROOT_DIR/docs/roadmap/012-codebase-consolidation-and-health.md"
)

for file in "${roadmap_files[@]}"; do
  require_heading "$file" "## Vision Alignment" || status=1
  require_heading "$file" "## Primary Tags" || status=1
  require_heading "$file" "## Target Envelope" || status=1
  require_heading "$file" "## Vision Target Delta" || status=1
done

guides=(
  "$ROOT_DIR/docs/guides/016-task-routing-precedence.md"
  "$ROOT_DIR/docs/guides/017-json-output-contracts.md"
  "$ROOT_DIR/docs/guides/018-doctor-explain-mode.md"
  "$ROOT_DIR/docs/guides/019-watch-init-migrate-phase-1.md"
  "$ROOT_DIR/docs/guides/020-dag-lock-policy-baseline.md"
  "$ROOT_DIR/docs/guides/021-quick-start-and-command-cookbook.md"
  "$ROOT_DIR/docs/guides/022-manifest-cookbook.md"
  "$ROOT_DIR/docs/guides/023-troubleshooting-and-failure-recipes.md"
  "$ROOT_DIR/docs/guides/024-ci-and-automation-recipes.md"
  "$ROOT_DIR/docs/guides/025-command-reference-matrix.md"
  "$ROOT_DIR/docs/guides/026-json-payload-examples.md"
)

for file in "${guides[@]}"; do
  require_heading "$file" "## Vision Alignment" || status=1
done

report_policy_files=(
  "$ROOT_DIR/docs/reports/README.md"
  "$ROOT_DIR/docs/guides/036-release-notes-authoring-template-and-examples.md"
)

for file in "${report_policy_files[@]}"; do
  if ! rg -q "Vision Target Delta" "$file"; then
    echo "[error] missing Vision Target Delta policy in ${file#$ROOT_DIR/}" >&2
    status=1
  fi
done

cutoff_policy_files=(
  "$ROOT_DIR/docs/reports/README.md"
  "$ROOT_DIR/docs/guides/029-docs-qa-checklist-and-validation.md"
)

for file in "${cutoff_policy_files[@]}"; do
  if ! rg -q "2026-03-06" "$file"; then
    echo "[error] missing forward-only cutoff date in ${file#$ROOT_DIR/}" >&2
    status=1
  fi
done

if ! "$ROOT_DIR/docs/scripts/check-doc-workflow-paths.sh"; then
  status=1
fi

if ! "$ROOT_DIR/docs/scripts/check-vision-index.sh"; then
  status=1
fi

if ! "$ROOT_DIR/docs/scripts/check-vision-next-task.sh"; then
  status=1
fi

if ! "$ROOT_DIR/docs/scripts/check-vision-next-task-regression.sh"; then
  status=1
fi

if [[ $status -ne 0 ]]; then
  exit $status
fi

echo "vision metadata check passed"
