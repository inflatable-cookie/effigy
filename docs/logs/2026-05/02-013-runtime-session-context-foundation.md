# g03.013 / 334 Runtime Session Context Foundation

Date: 2026-05-02
Roadmap: `g03.013`
Spec: `docs/specs/027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md`
Card: `334`

## Outcome

The first hardening slice for `g03.013` is landed.

Effigy now has one typed runtime/session context for:

- lease refresh policy
- bootstrap public-workspace stop-on-exit override

That typed context now governs the main runtime seams that previously depended
 on bootstrap-only env flags:

- bootstrap managed-run dispatch
- bootstrap embedded task dispatch
- public workspace handoff
- seeded shell ownership overlap
- routed container activation
- deferred container activation
- explicit `effigy exec`

The old ambient bootstrap env flags are no longer the primary control path for
 those ownership seams.

## Validation

- `cargo fmt --all`
- `cargo test -p effigy runtime_session_context::tests:: --lib -- --nocapture`
- `cargo test -p effigy host_container_lease::tests:: --lib -- --nocapture`
- `cargo test -p effigy workspace_session_ --lib -- --nocapture`
- `cargo test -p effigy run_bootstrap_with_cwd_starts_when_requested --lib -- --nocapture`
- `cargo test -p effigy activate_exec_surface_uses_repo_root_as_repo_override --lib -- --nocapture`
- `cargo test -p effigy inline_workspace_activation_skips_host_container_lease_refresh --lib -- --nocapture`
- `cargo test -p effigy run_manifest_task_decodelabs_bundle_defers_inside_container --lib -- --nocapture`
- `cargo test -p effigy run_manifest_task_decodelabs_bundle_defers_locally_inside_handoff_container --lib -- --nocapture`
- `./target/debug/effigy docs check-paths CHANGELOG.md docs/specs/027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md docs/roadmaps/g03/batch-cards/334-implement-typed-activation-and-session-context-foundation.md docs/roadmaps/g03/batch-cards/335-decide-post-typed-activation-and-session-context-foundation-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/013-runtime-session-context-and-runtime-ownership-hardening.md docs/roadmaps/g03/014-container-assembly-model-and-single-pass-compose-emission.md docs/roadmaps/g03/015-workspace-runtime-orchestrator-split-and-handoff-simplification.md docs/roadmaps/g03/016-container-and-runtime-error-taxonomy-and-diagnostics.md docs/roadmaps/g03/017-architecture-map-and-authority-surface-repair.md docs/roadmaps/g03/018-v1-runtime-hardening-proof-and-stress-matrix.md`

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: env-driven bootstrap/runtime ownership control -> typed runtime/session
  context carried through the main activation seams
- remains open: decide whether `g03.013` needs one more bounded ownership
  slice before handing off to the typed container assembly model in `g03.014`
