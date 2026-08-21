# 1087 - Guard And Close Documentation Coverage

Roadmap: [`../034-documentation-coverage-parity.md`](../034-documentation-coverage-parity.md)
Spec: [`../../../specs/archive/107-documentation-coverage-parity.md`](../../../specs/archive/107-documentation-coverage-parity.md)
Predecessor: [`1086`](./1086-audit-and-align-documentation-coverage.md)

Status: Complete
Owner: documentation validation and lane closeout
Created: 2026-08-21
Ready after: card `1086` gap inventory and repair

## Purpose

Turn the audit into durable evidence and proportional recurrence protection,
then close the strict lane without overstating what prose checks can prove.

## Work

- identify stable coverage relationships from card `1086` that can be checked
  deterministically without freezing prose or duplicating command authority
- add or extend focused tests/checks for those relationships, including skill
  copy parity and built-in discovery where appropriate
- run focused tests, docs QA, formatting, Clippy, and full Effigy QA
- publish one dated closeout log containing the evidence matrix, fixed gaps,
  already-covered surfaces, residual or blocked items, and exact validation
- mark cards, roadmap, strict spec, and front doors complete; archive strict
  spec `107` only after all acceptance criteria are met

## Acceptance

- [x] stable, mechanically detectable coverage relationships have regression
      checks
- [x] no test asserts arbitrary prose when a semantic or routing assertion is
      available
- [x] the closeout log supports every coverage and validation claim
- [x] focused checks and full QA pass
- [x] cards, roadmap, spec archive, generation/front-door state, and next task
      agree

## Validation

- focused help/config/skill/docs-policy tests
- `effigy qa`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Evidence Requirement

Close with one dated log under `docs/logs/2026-08/` and link it from both cards
and the roadmap.

Evidence:
[`2026-08/21-230738-documentation-coverage-parity-closeout.md`](../../../logs/2026-08/21-230738-documentation-coverage-parity-closeout.md)

## Stop Conditions

Stop if full validation exposes a behavior defect, if recurrence protection
would require a second registry, or if closing the lane would conceal an
unresolved in-scope gap.

## Next Task

Return the reviewable PR and evidence to the orchestrator. Do not merge.
