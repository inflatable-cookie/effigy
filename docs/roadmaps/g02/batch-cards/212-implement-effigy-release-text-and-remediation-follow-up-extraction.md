# 212 Implement Effigy Release Text And Remediation Follow Up Extraction

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining release text/projection and blocker-remediation layer out
of `src/runner/release_command.rs` so the release seam gets closer to an honest
interactive runner shell.

## In Scope

- target the still-local release text/projection layer around:
  - `render_release_status_text(...)`
  - `render_release_prepare_plan_text(...)`
  - `render_release_simulation_text(...)`
  - `render_release_prepared_text(...)`
  - `render_release_resume_text(...)`
  - `render_release_execute_plan_text(...)`
  - `render_release_verify_install_text(...)`
  - `render_release_executed_text(...)`
- move blocker-remediation shaping into `effigy-release`
- reduce `src/runner/release_command.rs` materially again
- keep the final interactive prompt loop and runner IO shell local

## Out Of Scope

- release execution
- release-closure lane work
- demo/container/docs-thread work
- broad shell cleanup outside the active release seam

## Acceptance Criteria

- the release text/projection surface no longer sits duplicated in
  `src/runner/release_command.rs`
- `crates/effigy-release/src/text.rs` becomes a real adopted surface instead of
  dead code
- the next move is a boundary decision, not another guessed release slice

## Validation

- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`213-decide-post-release-text-and-remediation-follow-up-boundary.md`](./213-decide-post-release-text-and-remediation-follow-up-boundary.md)
to decide whether the remaining release shell is now honest enough to pause or
still needs one more bounded cleanup slice.
