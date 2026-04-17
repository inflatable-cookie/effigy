# 079 Decide Demo Post-Projected-Output-Provenance Boundary

Status: superseded
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

This decision slice became stale after browser terminal fidelity testing showed
that the shipped browser live-terminal path still diverges materially from the
concurrent runner integration it was supposed to match.

## In Scope

- record why the old next-slice choice is no longer trustworthy
- preserve the no-nested-TUI rule
- hand the lane to one explicit recovery-backed ready card

## Out Of Scope

- further browser-terminal symptom patching
- generic process-manager UI
- multi-process browser panes by default
- embedding the concurrent TUI
- desktop-client work

## Acceptance Criteria

- stale authority is made explicit instead of left implied
- the lane is re-anchored on one honest ready card
- the next slice targets terminal-path convergence, not more symptom fixes

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Recovery Result

- superseded by [`080-implement-demo-browser-terminal-path-convergence.md`](./080-implement-demo-browser-terminal-path-convergence.md)
- reason:
  browser terminal fidelity is still broken after multiple bounded fixes, so
  the next honest move is to replace the browser’s custom live terminal
  integration with the same shared path the concurrent runner uses instead of
  choosing another superficial follow-up

## Next Task

Execute [`080-implement-demo-browser-terminal-path-convergence.md`](./080-implement-demo-browser-terminal-path-convergence.md)
to converge browser live terminal integration onto the concurrent-runner
terminal path before any further boundary decisions.
