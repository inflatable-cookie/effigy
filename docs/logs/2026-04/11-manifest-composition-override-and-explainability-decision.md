# 2026-04-11 - Manifest Composition Override And Explainability Decision

Roadmap: `g02.002`

## Summary

Closed the second bounded planning batch for manifest composition.

The important correction was practical: include-site override must not be
modeled as a coarse whole-fragment switch. In the common case, a repo will only
want to replace one value inside a larger composed fragment.

The contract now points at path-scoped override intent declared on include
entries, with whole-value replacement at the addressed path and explicit
conflict failure elsewhere.

## Changes

- completed the override/conflict/explainability ready card
- updated the roadmap with path-scoped override direction, illegal conflict
  classes, and minimum effective-manifest explainability requirements
- opened the next ready card for first implementation-slice and proof-boundary
  planning

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline `override was only framed as include-site intent and risked
  implying a coarse whole-fragment replacement model` -> current `override is
  still include-site owned, but path-scoped and explicit enough for practical
  one-value replacement without feature-local patch semantics`
- Remaining gap: `the first implementation slice and minimum proof boundary are
  still not explicitly chosen`

## Validation Performed

- command: `git diff --check`
  - result: pending
- command: `effigy qa:docs`
  - result: pending

## Next Task

Execute the active ready card in
`docs/specs/batch-cards/004-decide-composition-implementation-slice-and-proof-boundary.md`,
then leave the first implementation move explicit enough that composition can
start without under-scoping explainability or proof.
