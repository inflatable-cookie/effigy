# 104 Implement Consumer-Driven Distribution Gap Widening

Status: complete
Updated: 2026-04-15
Roadmap: `g02.005`
Spec: `docs/specs/archive/005-optional-distribution-surface-strict-lane.md`

## Objective

Widen the optional distribution surface only where the `convergence` consumer
proof exposed concrete remaining Effigy-shaped assumptions.

## In Scope

- remove or loosen `distribution validate-metadata` assumptions that require
  Effigy's exact release workflow layout
- remove or loosen `distribution first-publish` assumptions that require an
  Effigy-style CLI self-inspection path such as `--json tasks`
- keep the widening anchored to the specific consumer-proof evidence from
  `convergence`
- update the distribution guide and manifest contract docs to match the widened
  boundary honestly

## Out Of Scope

- broad channel abstraction beyond the named proof gaps
- `.github/workflows/` edits without explicit human approval
- claiming that every distribution consumer shape is now fully generic

## Acceptance Criteria

- the `convergence` proof gaps are either removed or moved behind explicit
  manifest policy
- the widened surface stays optional and composable
- the lane has an honest next boundary after the widening

## Validation

- rerun the relevant consumer-proof command path against `convergence`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute the follow-up decision card to decide whether the widened optional
distribution surface is now trustworthy enough to pause or whether one more
published-consumer proof is still warranted.
