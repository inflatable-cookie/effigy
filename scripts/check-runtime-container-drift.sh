#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

failures=0

check_no_unlisted_matches() {
  local name="$1"
  local pattern="$2"
  local paths="$3"
  local allowed_path_regex="$4"
  local matches
  local unexpected

  matches="$(rg --line-number --no-heading -e "$pattern" $paths || true)"
  if [[ -z "$matches" ]]; then
    printf '[ok] %s\n' "$name"
    return
  fi

  unexpected="$(printf '%s\n' "$matches" | awk -F: -v allow="$allowed_path_regex" '$1 !~ allow { print }')"
  if [[ -n "$unexpected" ]]; then
    printf '[fail] %s\n' "$name" >&2
    printf '%s\n' "$unexpected" >&2
    failures=$((failures + 1))
    return
  fi

  printf '[ok] %s (allowlisted debt only)\n' "$name"
}

check_no_unlisted_matches \
  'runner current_dir calls' \
  'std::env::current_dir\(' \
  'src/runner' \
  '^src/runner/exec_command/tests\.rs$'

check_no_unlisted_matches \
  'runner compose backend branching' \
  'resolve_compose_backend|ComposeBackend' \
  'src/runner crates/effigy-runtime/src' \
  '^(src/runner/doctor_ports\.rs|src/runner/exec_command/transport\.rs|src/runner/container_runtime_prep/prep\.rs|src/runner/container_command/lifecycle\.rs)$'

check_no_unlisted_matches \
  'runner raw container CLIs' \
  'Command::new\("(docker|colima|nerdctl)"' \
  'src/runner crates/effigy-runtime/src' \
  '^(src/runner/doctor_ports\.rs|src/runner/bootstrap_command/mod\.rs)$'

check_no_unlisted_matches \
  'compose_args callers outside manager adapters' \
  'compose_args\(' \
  'src/runner crates/effigy-runtime/src' \
  '^(crates/effigy-runtime/src/container_manager\.rs|src/runner/managed_shell\.rs|src/runner/demo_command/execute/task/selection\.rs|src/runner/exec_command/transport\.rs|src/runner/exec_command/transport/colima\.rs|src/runner/container_runtime_prep/prep\.rs|src/runner/container_runtime_prep/mod\.rs|src/runner/container_command/support\.rs|src/runner/deferral/run\.rs|src/runner/execute/pipeline/standard\.rs|src/runner/execute/pipeline/managed\.rs|src/runner/system_command/workspace_provisioning\.rs)$'

check_no_unlisted_matches \
  'legacy docker capture helper callers' \
  'run_docker_capture' \
  'src/runner crates/effigy-runtime/src' \
  '^$'

check_no_unlisted_matches \
  'legacy container exec capture callers' \
  'run_container_exec_capture' \
  'src/runner crates/effigy-rhai/src' \
  '^(src/runner/db_seed\.rs|src/runner/container_command/data\.rs|src/runner/container_command/lifecycle\.rs|src/runner/container_command/mod\.rs)$'

check_no_unlisted_matches \
  'rhai direct container exec capture bypass' \
  'run_container_exec_capture' \
  'crates/effigy-rhai/src src/runner/script_command' \
  '^$'

if (( failures > 0 )); then
  cat >&2 <<'MSG'

Runtime/container drift guard failed.
Either route the call through the current request/plan/manager surface, or add
a deliberate temporary allowance in scripts/check-runtime-container-drift.sh and
document it in docs/specs/052-drift-guards-and-architecture-proof-matrix-strict-lane.md.
MSG
  exit 1
fi
