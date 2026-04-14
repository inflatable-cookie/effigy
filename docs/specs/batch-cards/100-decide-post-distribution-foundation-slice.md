# 100 Decide Post-Distribution-Foundation Slice

Status: ready
Updated: 2026-04-14
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`

## Objective

Decide the next bounded slice after the manifest-driven distribution
foundation: widen manifest policy across more distribution commands, or prove
the new optional surface in one concrete consumer repo.

## In Scope

- assess whether the current `[distribution]` foundation is broad enough to
  justify a consumer proof
- assess whether `first-publish`, `write-summary`, and related commands still
  need one more internal manifest-policy batch first
- choose one explicit next card instead of widening the lane by implication

## Out Of Scope

- editing `.github/workflows/` without explicit human approval
- reopening the Rhai lane
- forcing distribution adoption in another repo during the decision batch

## Acceptance Criteria

- one explicit next slice is chosen
- the rationale for internal widening vs consumer proof is documented honestly
- one clear ready card exists after the decision

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

After this batch, either open the next internal distribution-policy widening
card or open one bounded consumer proof card for the optional distribution
surface.
