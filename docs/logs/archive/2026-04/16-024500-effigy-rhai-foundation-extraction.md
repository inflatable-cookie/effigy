# Effigy Rhai Foundation Extraction

Date: 2026-04-16
Owner: Platform

## Summary

`127` is complete.

Effigy now has a real `effigy-rhai` workspace crate. The Rhai runtime context
and host registration no longer sit entirely inside `src/runner/script_command.rs`.

## What Changed

- added [`crates/effigy-rhai`](../../../../crates/effigy-rhai/Cargo.toml)
- moved the first reusable Rhai host/runtime boundary there:
  - `ScriptContext`
  - Rhai env constants
  - script loading and env-backed arg loading helpers
  - stop-signal installation
  - host API registration
  - file/process/json/toml helpers
  - callback-based task / Effigy / container bridging
- reconnected [`src/runner/script_command.rs`](../../../../src/runner/script_command.rs)
  as a thinner adapter:
  - internal arg wiring stays in runner
  - Effigy-specific callbacks stay in runner
  - the scripting host no longer lives there wholesale
- added focused crate coverage for:
  - script loading
  - env-backed arg loading
  - generic task / Effigy / container callback wiring
  - JSON callback error surfacing

## Why Demo Is Next

After the release cluster and Rhai, the next largest still-interleaved product
surface is demos:

- `src/runner/demo_command.rs`
- `src/tui/demo_browser.rs`

That is now the next honest modularization seam.

## Current State

- active strict lane: `g02.010`
- active ready card: `128`
- queued release card: `115`

## Validation

- `cargo test -p effigy-rhai`
- `cargo test run_manifest_task_run_array_rhai_steps_support_in_process_effigy_dispatch --lib`
- `cargo test run_manifest_task_run_array_rhai_steps_support_container_helpers --lib`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned Rhai scripting host`
  to `workspace-owned Rhai runtime and host-registration foundation with runner adapters`
- remains open:
  - demo foundation extraction
  - further modularization beyond the already-shipped crate slices
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`128-implement-effigy-demo-foundation-extraction.md`](../../../specs/batch-cards/128-implement-effigy-demo-foundation-extraction.md)
to move the next largest still-interleaved domain cluster out of `runner`.
