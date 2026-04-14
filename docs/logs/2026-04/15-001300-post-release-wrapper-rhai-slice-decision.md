# Post-Release-Wrapper Rhai Slice Decision

Date: 2026-04-15
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`
Batch: `095-decide-post-release-wrapper-rhai-slice`

## Decision

Do one more Effigy-only wrapper-convergence batch.

The Rhai surface is technically broad enough for an external pilot, but the
chosen first pilot repo is still explicitly unsafe to touch. The honest next
move is one final Effigy-only batch that cleans up the remaining internal
wrapper boundary instead of pretending the lane is blocked on product design.

## Why

The current Effigy surface now covers:

- manifest-backed Rhai steps
- task glue
- demo runners
- stop-aware long-running lifecycle
- compatibility release wrappers

That is enough capability proof. The constraint is execution safety, not
missing Rhai features.

There is still one meaningful internal cleanup move left:

- separate shell scripts that should become minimal Rhai launchers from scripts
  that are honest permanent shell boundaries because they drive external
  binaries, release side effects, or local-machine concerns

That keeps the lane moving without widening into another repo prematurely.

## Chosen Boundary

Next batch:

- stays inside Effigy
- converges the remaining wrapper boundary
- leaves the lane ready either to pause cleanly or to reopen the external pilot
  when the repo boundary is safe again

Not chosen:

- reopening the first external pilot while it is still unsafe
- broad host-API replanning
- Jetstream work

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Movement:
  - clarified that the blocker is repo-boundary safety, not missing Rhai
    capability
  - chose one last Effigy-only cleanup slice instead of a stale external move
- Remaining open:
  - whether the Effigy dogfooding lane should pause after wrapper convergence
    until the external pilot boundary is safe again
