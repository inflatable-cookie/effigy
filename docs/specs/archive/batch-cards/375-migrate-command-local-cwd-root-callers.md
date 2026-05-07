# 375 - Migrate Command-Local Cwd Root Callers

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Move the first runner cwd/root callers behind `EffigyRuntimeContext` so the
new context crate becomes the normal dispatch path, not just a captured side
object.

## Scope

- make public runner entrypoints capture or receive `EffigyRuntimeContext`
- keep legacy cwd/root helpers as wrappers only where migration is not complete
- migrate command-local `current_working_dir()` and `resolve_repo_root()` callers
  that sit on the direct CLI dispatch path
- inventory Rhai script entrypoints that still receive only cwd/repo root and
  mark what they need from `EffigyRuntimeContext`
- add the first lightweight drift guard for direct `std::env::current_dir()` in
  `src/runner/**`
- leave container backend and task execution request work to later cards

## Exit Condition

This card is complete when direct CLI dispatch no longer recalculates cwd/root
after context capture, affected tests pass, and remaining runner-local path
probes are either migrated or inventoried into the next card. Rhai must be
called out explicitly in that inventory because it is the current known
host/container path failure mode.

## Closeout

Direct runner dispatch now installs an active `EffigyRuntimeContext` while a
command runs. Existing runner cwd/root helpers read that context before falling
back to process-local discovery, which lets command modules migrate without
signature churn.

Migrated in this card:

- public runner entrypoint fallback now captures `EffigyRuntimeContext`
- `current_working_dir()` reads active context first
- `resolve_repo_root()` reuses the active context target when cwd/repo override
  match
- `resolve_command_root()` reads active context first
- deferral inside-container local handoff uses captured context cwd and
  container handoff state
- deploy command cwd lookup uses the context-backed helper
- direct runner production drift guard for `std::env::current_dir()` is clean

Rhai inventory:

- `effigy-rhai::ScriptContext` still receives `cwd` and `repo_root` only
- first-party scripts that do path-sensitive container work need read-only
  runtime context exposure
- DecodeLabs mysql seeding is the reference migration for the queued Rhai
  execution helper card

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-context-target cargo test -p effigy-context`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy --lib command_context -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `rg -n "std::env::current_dir\\(" src/runner -g '*.rs' -g '!**/tests.rs' -g '!**/tests/**'`

## Next Task

Promote and implement card `376`.
