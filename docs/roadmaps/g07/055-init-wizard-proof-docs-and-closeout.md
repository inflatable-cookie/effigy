# g07.055 - Init Wizard Proof Docs And Closeout

Status: Complete
Depends on: `g07.054`

## Goal

Finish the init setup-wizard lane with proof, docs, contracts, and closeout
discipline.

## Scope

- add CLI/help coverage for TTY vs non-TTY init behavior
- add JSON contract coverage for checklist output and action execution output
- add focused integration coverage for:
  - baseline-only repos
  - repos with graph state
  - repos with secrets config
  - repos with bundle config
  - repos with package.json task wrappers
- update README, quick start, command reference, and agent adoption docs
- close the lane honestly with any deferred setup jobs called out explicitly

## Guardrails

- do not claim the wizard covers a setup job unless proof exists
- keep docs honest about what is guidance-only versus automated
- avoid turning init into a release/deploy supervisor narrative

## Acceptance Criteria

- TTY wizard behavior is proven
- checklist JSON and action-execution JSON are proven
- docs explain when plain `effigy init` prompts and when it does not
- the lane closes with no stale ready card

## Next Task

No active ready card until the closeout card finishes.
