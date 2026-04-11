# 2026-04-11 - Manifest Composition Contract Shape Decision

Roadmap: `g02.002`

## Summary

Closed the first bounded planning batch for manifest composition.

The contract direction is now explicit:

- Effigy should use `[manifest].include` as the root composition surface
- included files should be partial manifest fragments
- paths should resolve relative to the including file
- nested composition is allowed, but cycles and unreadable fragments fail hard
- override intent belongs at the include-site, not hidden inside feature data

This is enough to stop future feature lanes from inventing feature-local
external config loading. It is not yet enough to implement composition, because
override semantics, conflict boundaries, and explainability still need one more
bounded planning batch.

## Changes

- recorded the `g02.002` Batch `02.1` contract decision in the roadmap
- completed the first ready card
- opened the next ready card for override/conflict/explainability
- refreshed currentness surfaces so the active batch is now the override lane

## Vision Target Delta

- Primary tags: `CONTRACT`, `MAINT`, `OPERATE`
- Movement: baseline `manifest composition existed only as a broad idea with
  include/require/import ambiguity` -> current `root composition shape is
  explicit enough for later feature planning: [manifest].include, partial
  fragments, file-relative resolution, include-site override intent`
- Remaining gap: `override semantics, conflict boundaries, and effective-manifest
  explainability are still undecided`

## Validation Performed

- command: `git diff --check`
  - result: pending
- command: `effigy qa:docs`
  - result: pending

## Next Task

Execute the active ready card in
`docs/specs/batch-cards/003-decide-override-conflict-and-explainability.md`,
then leave the override/explainability move explicit enough that implementation
planning can start without silent merge folklore.
