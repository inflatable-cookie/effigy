# 616 - Close State Stack Release Slice

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Decide whether the state-stack framework should be the next Effigy release
slice, based on the Acowtancy proof and remaining framework gaps.

## Scope

- summarize the implemented state-stack surface
- record the Acowtancy proof result and the routed container env fix
- identify release-blocking gaps, if any
- ensure docs and JSON examples describe the actual boundary
- leave release execution to a human-owned release thread

## Non-Goals

- no release prepare or execute commands
- no additional app-specific migration semantics
- no production Acowtancy migration execution

## Exit Condition

This card is complete when the lane has a clear hold/continue recommendation
for the next Effigy release boundary.

## Result

Hold this as the next Effigy release boundary.

The implemented state-stack surface is now large enough to be useful and small
enough to be defensible:

- `effigy state plan` reads standalone state manifests and composed `[state]`
  config through normal manifest loading
- `effigy state apply` has plan-only safety by default and first adapters for
  task, artifact, and SQL layers
- `effigy state capture` has explicit task, local artifact staging, and OCI
  publish boundaries
- capture tasks receive a versioned context file plus environment aliases
- plan, apply, and capture reports write latest pointers and timestamped
  history files
- `effigy state history` provides read-only lookup over report files

The Acowtancy proof confirmed the app boundary:

- the stack can model structure, canonical migrated seed, and legacy content as
  ordered layers
- repo-owned capture work can run behind an Effigy context contract
- generated payloads can be staged as replayable artifact material
- Acowtancy/Farmyard still own transforms, media binding, conflict detection,
  and reconciliation

The proof also exposed and fixed a real framework gap: dynamic task environment
overrides were not forwarded through routed workspace-container handoff. The
fix keeps capture context available to repo-owned tasks even when routing
enters a nested workspace container.

## Release Recommendation

Do not add rebase execution, post-go-live sync, app-specific apply hooks, or a
durable lineage database before the next release. Those are larger semantic
surfaces and should wait until Acowtancy has rebased real migration code onto
this first contract.

The next release should present this as an initial state-stack framework:

- app-agnostic stack composition
- explicit plan/apply/capture commands
- artifact-backed capture and replay seams
- report history for operator visibility
- repo-owned hooks for domain work

Remaining gaps are non-blocking for this release slice:

- no automatic conflict/rebase execution
- no app-specific payload schema
- no retention/pruning for report history
- no production sync daemon
- no guarantee that released v0.5.0 consumers can parse composed `[state]`
  config; consumers should use standalone state manifests until this release is
  installed

## Validation

- focused state-stack tests
- routed task env forwarding test
- docs and JSON contract checks
- `git diff --check`

Validated:

- `cargo fmt --all -- --check`
- `cargo test -p effigy --lib build_routed_task_exec_args_forwards_task_env_to_handoff -- --nocapture`
- `cargo test --test cli_output_tests state_command_tests::cli_state_capture_yes_runs_task_before_staging -- --nocapture`
- `cargo run --bin effigy -- docs check-paths ...`
- `git diff --check`
- Acowtancy installed `effigy tasks`
- Acowtancy dev-binary `effigy --json state plan state/acowtancy-uat.toml`
- Acowtancy root and ledger `git diff --check`

## Next Task

Hand off to the release-prep thread. Release execution remains human-owned.
