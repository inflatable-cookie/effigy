# 2026-04-11 - Manifest Composition Implementation Slice Decision

Roadmap: `g02.002`

## Summary

Closed the implementation-boundary planning batch for manifest composition.

The next move is now explicit: implement the narrowest composition foundation
that is still honest:

- load composed manifests
- enforce path-scoped override and conflict rules
- provide one native inspection surface under `effigy config`
- prove the feature on one cross-feature split rather than on tasks alone

## Changes

- completed the implementation-slice ready card
- recorded the first implementation/proof boundary in the roadmap
- opened the first implementation-ready card for composed-manifest loading and
  inspection foundation

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline `composition contract was defined, but the first shipping
  scope was still broad and implied` -> current `the first shipping slice is
  explicit: composed load + conflict enforcement + minimal inspection + one
  cross-feature proof`
- Remaining gap: `the implementation itself is not yet built`

## Validation Performed

- command: `git diff --check`
  - result: pending
- command: `effigy qa:docs`
  - result: pending

## Next Task

Execute the active ready card in
`docs/roadmaps/g02/batch-cards/005-implement-composed-manifest-loading-and-inspection-foundation.md`,
then leave the next move explicit as either composition hardening or activation
of `g02.003` planning.
