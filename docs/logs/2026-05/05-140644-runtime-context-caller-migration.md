# 2026-05-05 - Runtime Context Caller Migration

## Summary

Completed card `375` for the first direct runner context migration.

## Changed

- added an active runner `EffigyRuntimeContext` scope
- made runner cwd/root helpers consume the active context first
- made public runner fallback dispatch capture context instead of reading cwd in
  dispatch
- migrated deploy cwd lookup through the context-backed helper
- migrated deferral inside-container local handoff to captured context state
- added focused command-context tests
- promoted card `376` for the Rhai runtime/execution helper design

## Rhai Inventory

`effigy-rhai::ScriptContext` still receives only `cwd` and `repo_root`. The
DecodeLabs mysql seed path shows that Rhai needs the universal runtime context
and an execution helper that can route `run_in = "container"` without scripts
choosing between `process::run(...)` and `container::exec(...)`.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-context-target cargo test -p effigy-context -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `rg -n "std::env::current_dir\\(" src/runner -g '*.rs' -g '!**/tests.rs' -g '!**/tests/**'`

## Next

Implement card `376`.
