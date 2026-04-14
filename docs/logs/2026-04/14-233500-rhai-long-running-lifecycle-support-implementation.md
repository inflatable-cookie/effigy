# Rhai Long-Running Lifecycle Support Implementation

Date: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`
Batch: `091-implement-rhai-long-running-lifecycle-support`

## Summary

Implemented the bounded Rhai lifecycle helpers needed for one honest
long-running first-party script, migrated `lifecycle-window` off its shell loop,
and adjusted interactive Rhai-backed demo transport so stop-aware cleanup can
finish on macOS without depending on the PTY wrapper path.

## What Landed

- extended the Rhai host API with:
  - `stop_requested()`
  - `process_id()`
  - `sleep_ms(milliseconds)`
  - `append_file(path, contents)`
- added signal-aware stop tracking for Rhai scripts
- migrated `lifecycle-window` to `scripts/rhai/run-lifecycle-window.rhai`
- removed the old shell-backed `scripts/demo/run-lifecycle-window.sh`
- changed interactive Rhai-backed runs to prefer attached-stream transport so
  stop-aware cleanup can complete cleanly
- updated the Rhai guide and changelog to reflect the shipped lifecycle surface

## Validation

- `cargo run --bin effigy -- demo run lifecycle-window`
- `cargo run --bin effigy -- demo stop lifecycle-window`
- verified `.effigy/demo/artifacts/lifecycle-window/status.txt` ends as
  `terminated at ...`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Residual Notes

- this batch intentionally proved one bounded long-running script lifecycle, not
  a general process-supervision API inside Rhai
- the next real decision is whether Effigy dogfooding has now gone far enough
  to widen into another repo

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `ADOPT`
- Movement:
  - `lifecycle-window` moved from shell-backed proof to Rhai-backed proof
  - the Rhai host API moved from short-lived glue only to one honest
    stop-aware long-running lifecycle
  - interactive Rhai-backed demos now use the transport path that preserves
    graceful shutdown cleanup on macOS
- Remaining open:
  - whether the next Rhai slice should stay Effigy-only or widen into the first
    external pilot
