# 084 - Codebase Lean-Down Strict Lane

Roadmap: [`g06.001`](../roadmaps/g06/001-codebase-lean-down-suite.md)
Contracts:
- [`027-state-domain-extraction-contract.md`](../contracts/027-state-domain-extraction-contract.md)
- [`030-low-risk-deduplication-contract.md`](../contracts/030-low-risk-deduplication-contract.md)
- [`031-artifact-and-crate-boundary-contract.md`](../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Execute the first post-`v0.7.0` codebase lean-down tranche without turning it
into a broad rewrite.

This lane exists to remove real ownership and duplication debt while preserving
released behavior and keeping release/test confidence high.

## Lane Posture

Posture: `strict-active`

This lane is executable because the roadmap suite is written, the major weight
areas are already known, and the first slices are bounded and measurable.

## Hard Boundaries

- no release protocol weakening
- no `.github/workflows/` edits unless the user explicitly asks again
- no speculative crate merges
- no syntax golf or readability regression in pursuit of lower LOC
- no breaking JSON or CLI contract changes unless a later card explicitly opens
  one
- no edits under `external/`
- no broad rewrite of state, release, or runner subsystems

## Execution Order

1. `800` complete: lane opened and ready chain wired
2. `801` complete: baseline size, duplication, and god-file metrics
3. `802` complete: state command config owner extraction
4. `803` complete: release lib domain split and reduction
5. `804` complete: shared fixture and test-support convergence
6. `805` complete: CLI help and rendering deduplication
7. `806` complete: typed contract shape reuse and JSON builder reduction
8. `807` complete: compatibility branch audit and deletion
9. `808` complete: runner-private domain logic reduction
10. `809` complete: close lane with before/after proof

## Ready Chain

- `800` is complete
- `801` is complete
- `802` is complete
- `803` is complete
- `804` is complete
- `805` is complete
- `806` is complete
- `807` is complete
- `808` is complete
- `809` is complete

## Auto-Continuation Envelope

Auto-start is enabled for this lane while:

- the previous card closes green
- the next card still has a bounded write surface
- no contract break becomes necessary to realize the reduction
- the change still reduces real ownership or duplication rather than just
  rearranging code

Stop and replan if implementation discovers:

- a target lane needs a public contract break
- a proposed abstraction increases indirection without deleting enough code to
  justify it
- state or release work wants a redesign rather than a bounded split
- compatibility deletion collides with released-surface guarantees

## Acceptance

This lane is complete when:

- baseline size and duplication metrics are recorded
- the largest owner seams have materially clearer boundaries
- fixture/help/render/JSON-shape duplication is reduced where evidence supports
  it
- dead compatibility branches are deleted where proof permits
- remaining large modules and retained duplication are explicitly justified
- front doors point at the next active queue or closeout state

## Next Task

None. Lane `084` is closed.
