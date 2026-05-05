# 375 - Migrate Command-Local Cwd Root Callers

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Move the first runner cwd/root callers behind `EffigyRuntimeContext` so the
new context crate becomes the normal dispatch path, not just a captured side
object.

## Scope

- make public runner entrypoints capture or receive `EffigyRuntimeContext`
- keep legacy cwd/root helpers as wrappers only where migration is not complete
- migrate command-local `current_working_dir()` and `resolve_repo_root()` callers
  that sit on the direct CLI dispatch path
- add the first lightweight drift guard for direct `std::env::current_dir()` in
  `src/runner/**`
- leave container backend and task execution request work to later cards

## Exit Condition

This card is complete when direct CLI dispatch no longer recalculates cwd/root
after context capture, affected tests pass, and remaining runner-local path
probes are either migrated or inventoried into the next card.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-context-target cargo test -p effigy-context`
- targeted runner context tests
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- drift guard command for direct runner `std::env::current_dir()`

## Next Task

Implement this card.
