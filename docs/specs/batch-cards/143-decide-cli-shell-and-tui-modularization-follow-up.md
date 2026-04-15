# 143 Decide CLI Shell And TUI Modularization Follow-Up

Status: ready
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining pre-`v0.3` modularization work should continue
through a CLI-shell crate, a TUI/runtime crate, or both in sequence.

## In Scope

- assess the remaining `src/` hotspots after the current domain extractions
- classify the CLI parse/help/command-model cluster honestly
- classify the TUI/browser/runtime cluster honestly
- choose the next bounded extraction batch instead of pausing on a soft claim

## Out Of Scope

- executing the release lane in the same batch
- broad cleanup without a crate-boundary decision
- consumer rollout work

## Acceptance Criteria

- the next remaining modularization seam is explicit
- the release lane posture is honest
- one clear next batch is left ready

## Validation

- docs/state surfaces updated honestly
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decide whether the next real seam is CLI shell extraction, TUI/runtime
extraction, or one last tighter architectural batch before `v0.3`.
