# 2026-04-11 - g02 Post-Bootstrap Roadmap Split

Roadmap: `g02`

## Summary

Captured the next product-cycle shape after the active bootstrap lane.

This batch does not change the active strict execution lane for `g02.001`.
Instead, it makes the post-bootstrap planning sequence explicit:

- `g02.002` for a general manifest composition and override contract
- `g02.003` for the demo harness model, runner semantics, and browser-facing
  contract

The key decision is structural: external config loading must not become a
demo-specific feature. Effigy needs one manifest composition model first, with
explicit override behavior planned in, so later feature lanes can scale without
inventing their own import semantics.

## Changes

- added `g02.002` roadmap for manifest composition and override semantics
- added `g02.003` roadmap for first-class demo harness design
- updated `g02` and top-level roadmap indexes to show the post-`g02.001`
  sequence explicitly
- updated generation history and log currentness surfaces accordingly

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline `bootstrap is the only explicit g02 lane and the next
  product cycle is implied` -> current `post-bootstrap roadmap split is
  explicit, with manifest composition separated from the demo harness model`
- Remaining gap: `g02.001` still needs to resolve its active ready-card decision
  before either planned lane becomes active work`

## Validation Performed

- command: `git diff --check`
  - result: pending
- command: `effigy qa:docs`
  - result: pending

## Next Task

Finish the active `g02.001` ready-card decision first, then use the result to
decide whether `g02.002` or a narrower bootstrap closeout batch becomes the next
active strict lane.
