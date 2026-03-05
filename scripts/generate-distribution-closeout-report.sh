#!/usr/bin/env bash
set -euo pipefail

# Wrapper policy:
# - Compatibility entrypoint retained for CI/release/docs tooling.
# - Prefer cargo/Effigy command entrypoints for operator-driven runs.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG=""
ARTIFACTS_DIR=""
OUTPUT_PATH=""
OWNER="Effigy"
EXPECT_HOMEBREW=0
EXPECT_HOMEBREW_EXPLICIT=0

usage() {
  cat <<'USAGE'
Usage:
  ./scripts/generate-distribution-closeout-report.sh --tag <vX.Y.Z> --artifacts-dir <dir> [options]

Options:
  --tag <tag>              Release tag used for first-publish execution (required)
  --artifacts-dir <dir>    Directory containing step logs from check-distribution-first-publish.sh (required)
  --output <path>          Output report path (default: docs/reports/<date>-distribution-acceptance-closeout-<tag>.md)
  --owner <name>           Report owner label (default: Effigy)
  --expect-homebrew        Require Homebrew channel logs in artifact validation
  --help                   Show this help
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      TAG="$2"
      shift 2
      ;;
    --artifacts-dir)
      ARTIFACTS_DIR="$2"
      shift 2
      ;;
    --output)
      OUTPUT_PATH="$2"
      shift 2
      ;;
    --owner)
      OWNER="$2"
      shift 2
      ;;
    --expect-homebrew)
      EXPECT_HOMEBREW=1
      EXPECT_HOMEBREW_EXPLICIT=1
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

if [[ -z "$TAG" || -z "$ARTIFACTS_DIR" ]]; then
  echo "[error] --tag and --artifacts-dir are required" >&2
  usage >&2
  exit 1
fi

if [[ ! "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$ ]]; then
  echo "[error] tag must match vX.Y.Z format: $TAG" >&2
  exit 1
fi

if [[ ! -d "$ARTIFACTS_DIR" ]]; then
  echo "[error] artifacts directory not found: $ARTIFACTS_DIR" >&2
  exit 1
fi

summary_path="$ARTIFACTS_DIR/distribution-summary.env"
if [[ "$EXPECT_HOMEBREW_EXPLICIT" -eq 0 && -f "$summary_path" ]]; then
  summary_homebrew="$(awk -F= '$1=="HOMEBREW_EXECUTED"{print $2}' "$summary_path" | tail -n1)"
  if [[ "$summary_homebrew" == "1" ]]; then
    EXPECT_HOMEBREW=1
  fi
fi

validate_args=(--artifacts-dir "$ARTIFACTS_DIR")
if [[ "$EXPECT_HOMEBREW" -eq 1 ]]; then
  validate_args+=(--expect-homebrew)
fi
"$ROOT_DIR/scripts/validate-distribution-artifacts.sh" "${validate_args[@]}"

log_files=()
while IFS= read -r log_file; do
  log_files+=("$log_file")
done < <(find "$ARTIFACTS_DIR" -maxdepth 1 -type f -name '*.log' | sort)

if [[ "${#log_files[@]}" -eq 0 ]]; then
  echo "[error] no .log files found in artifacts directory: $ARTIFACTS_DIR" >&2
  exit 1
fi

log_summary_lines=()
for log_file in "${log_files[@]}"; do
  log_basename="$(basename "$log_file")"
  log_summary_lines+=("- $log_basename")
done

has_homebrew_logs="false"
if compgen -G "$ARTIFACTS_DIR/*homebrew*.log" > /dev/null; then
  has_homebrew_logs="true"
fi

today="$(date +%F)"
if [[ -z "$OUTPUT_PATH" ]]; then
  sanitized_tag="${TAG#v}"
  sanitized_tag="${sanitized_tag//./-}"
  OUTPUT_PATH="$ROOT_DIR/docs/reports/${today}-distribution-acceptance-closeout-${sanitized_tag}.md"
fi

mkdir -p "$(dirname "$OUTPUT_PATH")"

cat > "$OUTPUT_PATH" <<EOF2
# Distribution Acceptance Closeout (${TAG})

Date: ${today}
Owner: ${OWNER}
Related roadmap: docs/roadmap/backlog/distribution-channels.md

## Scope

- Capture publish-cycle distribution evidence from artifact logs.
- Record acceptance-closeout outcomes for tag ${TAG}.

## Inputs

- release tag: ${TAG}
- artifacts directory: ${ARTIFACTS_DIR}
- artifacts summary: ${summary_path}

## Evidence Logs

$(printf '%s\n' "${log_summary_lines[@]}")

## Outcomes

- First-publish script artifacts were captured and linked for closeout evidence.
- Rust-native install path and CI-style install validation evidence are included in this report via logs.
- Homebrew evidence included: ${has_homebrew_logs}.

## Risks / Follow-ups

- If any expected channel log is missing, rerun ./scripts/check-distribution-first-publish.sh --tag ${TAG} --artifacts-dir <dir> before final sign-off.
- External channel state (crates.io availability, Homebrew tap CI, network reliability) still determines final release readiness.

## Next Batch Recommendation

- Reconcile acceptance checkboxes in docs/roadmap/backlog/distribution-channels.md against this evidence and publish release sign-off notes.
EOF2

echo "[ok] wrote report: $OUTPUT_PATH"
