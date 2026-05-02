# 02 015 Workspace Provisioning Split Foundation

Date: 2026-05-02
Roadmap: `g03.015`
Spec: `docs/specs/029-workspace-runtime-orchestrator-split-and-handoff-simplification-strict-lane.md`
Batch: `342`

## What Landed

- added a dedicated `workspace_provisioning` owner under
  `src/runner/system_command/`
- moved workspace provisioning and prep there:
  - Linux effigy artifact resolution and install
  - workspace permission preparation
  - local-versus-download artifact source handling
  - staging/install helper ownership
- updated the public handoff path so provisioning is invoked as one combined
  step instead of caller-local effigy-then-permissions sequencing
- moved the provisioning tests to target `workspace_provisioning` directly
  instead of leaning on compatibility shims in `workspace.rs`

## Validation

- `cargo test -p effigy workspace_session_ --lib -- --nocapture`
- `cargo test -p effigy workspace_artifact_source_ --lib -- --nocapture`
- `cargo test -p effigy workspace_effigy_ --lib -- --nocapture`
- `cargo test -p effigy workspace_handoff_preparation_ --lib -- --nocapture`
