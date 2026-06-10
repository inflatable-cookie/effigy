# 02 015 Workspace Session Orchestrator Foundation

Date: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`
Batch: `340`

## What Landed

- added a dedicated `workspace_session` owner under
  `src/runner/system_command/`
- moved the public workspace/session lifecycle orchestration there:
  - policy load
  - runtime prep
  - handoff prep
  - ownership classification
  - shell handoff
  - cleanup decision and teardown
- kept lower-level shell/install/permission helpers in `workspace.rs` for now

## Validation

- `cargo test -p effigy workspace_session_ --lib -- --nocapture`
- `cargo test -p effigy run_bootstrap_with_cwd_starts_when_requested --lib -- --nocapture`
- `cargo test -p effigy bootstrap_started_workspace_session_forces_stop_on_exit_even_for_ready_adopted_runtime --lib -- --nocapture`
