# 102 Decide Post Distribution Policy Widening Slice

Status: archived
Updated: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/archive/005-optional-distribution-surface-strict-lane.md`

## Objective

Decide whether the widened optional `[distribution]` contract is now honest
enough for one bounded consumer-proof adoption batch, or whether one final
internal policy gap still needs to be closed first.

## In Scope

- assess the current manifest-driven distribution boundary after `101`
- compare the remaining Effigy-shaped assumptions against the goal of optional
  cross-repo adoption
- choose the next bounded move for `g02.005`

## Out Of Scope

- forcing a consumer repo to adopt the distribution surface during this card
- widening every distribution channel variation in one decision batch
- `.github/workflows/` edits without explicit human approval

## Acceptance Criteria

- the next `g02.005` move is explicit
- any remaining internal policy gaps are named concretely if consumer proof is
  still premature
- the strict lane stays trustworthy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this decision, run one bounded consumer-proof adoption batch for the
optional distribution surface.
