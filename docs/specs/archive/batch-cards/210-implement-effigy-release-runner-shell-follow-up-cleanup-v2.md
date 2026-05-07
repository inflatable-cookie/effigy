# 210 Implement Effigy Release Runner Shell Follow Up Cleanup V2

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Reduce the next largest still-open `/src` shell by cleaning up
`src/runner/release_command.rs` after the container seam pause.

## In Scope

- target the remaining release runner shell around:
  - interactive prepare/execute/resume review loops
  - prompt and section-browser IO
  - release-specific progress/error adaptation
  - final runner-side apply/review wiring still sitting outside
    `effigy-release`
- reduce `src/runner/release_command.rs` materially again
- keep work bounded to runner-shell cleanup rather than reopening broad
  release-domain extraction by default
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution
- docs-thread cleanup work
- container-design work
- unrelated cleanup outside the active modularization lane

## Acceptance Criteria

- `src/runner/release_command.rs` no longer bundles the bulk of the chosen
  release shell cluster inline
- the remaining release shell is described honestly after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`211-decide-post-release-runner-shell-follow-up-cleanup-v2-boundary.md`](./211-decide-post-release-runner-shell-follow-up-cleanup-v2-boundary.md)
to decide whether the remaining release shell is now honest enough to pause or
still justifies one more bounded follow-up.
