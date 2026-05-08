# 073 Decide Demo Post-Browser-Live-Concurrent-Runner-Session-Parity Boundary

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after browser-owned live attached terminal
sessions reached bounded parity for single-process concurrent-runner-backed
interactive demos, while keeping the lane demo-scoped and still bounded away
from nested TUI embedding, generic process-manager work, and fresh browser
churn unless it clearly earns the next slot.

## In Scope

- decide whether the next value belongs in:
  - one more bounded browser/runtime parity follow-up
  - a runner-owned follow-up for richer concurrent-runtime truth
  - a pause from browser-terminal work now that the bounded live path covers
    both run-backed and single-process concurrent-runner demos
- preserve the no-nested-TUI rule
- leave one explicit ready card

## Out Of Scope

- implementing the next slice
- generic process-manager UI or multi-process demo browser controls
- embedding the concurrent TUI inside `effigy demo browser`
- desktop-client work

## Acceptance Criteria

- the next bounded slice after bounded browser live-session parity is explicit
- the decision stays demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Decision

- do not spend the next slot on more browser chrome or controls
- do not widen into multi-process browser tabs or nested TUI embedding
- do not pause the lane yet
- the next bounded slice is runner-owned richer concurrent-runtime truth for
  multi-process demos
- specifically: add demo-scoped backend/session facts that let clients know
  when a concurrent-runner demo is single-terminal, projected-multi-process,
  or otherwise not eligible for one live attached browser terminal

## Why

- browser parity is good enough now for the honest single-terminal cases
- the main remaining mismatch is not presentation, it is richer runtime truth
  for concurrent demos that do not fit one terminal
- more browser work now would force semantics through UI again
- a runner-owned projection-shape contract keeps later browser follow-up honest
  without reopening nested TUI or generic process-manager drift

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh operator intent

## Next Task

Execute
`074-implement-demo-concurrent-runtime-projection-shape-contract.md` to add
runner-owned projection-shape facts for richer concurrent-runner demos before
any later multi-process browser follow-up is considered.
