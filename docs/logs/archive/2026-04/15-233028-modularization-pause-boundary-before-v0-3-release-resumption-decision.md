# Modularization Pause Boundary Before v0.3 Release Resumption Decision

Date: 2026-04-15
Roadmap: `g02.010`
Card: `142`

## Summary

The modularization lane can now pause for pre-`v0.3` purposes.

The repo still has large command files, but the remaining weight is now mostly
shell, render, TUI, git, and process orchestration over real extracted domain
crates instead of another obvious reusable product seam.

## Decision

Pause `g02.010` and resume `g02.007`.

Move `115` back to `ready` as the active next batch.

## Why This Boundary Is Honest

The shared backbone is now real:

- `effigy-core`
- `effigy-tasks`
- `effigy-manifest`

The main product/domain seams are now real too:

- `effigy-containers`
- `effigy-distribution`
- `effigy-release`
- `effigy-rhai`
- `effigy-demo`
- `effigy-docs-policy`
- `effigy-env`
- `effigy-doctor`

The remaining runner-local weight is concentrated in command-shell files that
now mostly adapt those crates rather than defining another still-unextracted
domain.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: `active modularization lane with one final boundary decision pending` -> `paused modularization lane with release closure active again`
- Remaining gap: `v0.3` release closure and later post-release modularization follow-up if a new seam justifies reopening `g02.010`

## Validation Performed

- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Next Task

Execute [`115-implement-effigy-distribution-release-closure.md`](../../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md).
