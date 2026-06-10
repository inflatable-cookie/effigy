# 2026-04-17 03:30:00 BST — Effigy Process Subsystem Extraction

## Summary

Moved the process supervision subsystem out of the root crate and into a new
`effigy-process` crate. `g02.017` queue job #4 shipped.

`src/process_manager.rs` + `src/process_manager/**` are gone. The subsystem
lives at `crates/effigy-process/src/**` with its own integration test harness
at `crates/effigy-process/tests/integration.rs`.

## Why This Batch

Per `g02.017` queue job #4, process supervision is a real cross-cutting
subsystem, not a command-local helper. It was imported by 22+ call sites
across `src/runner/**` and `src/tui/multiprocess/**`. The roadmap job
explicitly warned against folding it into `effigy-exec` if that would cause
artificial mixing — and `effigy-exec` is container-routing (routing, cwd,
detection, health), which is a disjoint concern. A dedicated crate was the
honest call.

## What Changed

- added `crates/effigy-process/` with:
  - `lib.rs` — `ProcessSpec`, `ProcessEvent`, `ProcessEventKind`,
    `ProcessSupervisor`, `ProcessManagerError`, `ShutdownProgress`, event
    pump + exit diagnostics entry points
  - `diagnostics.rs`, `signal.rs`, `streams.rs` — low-level helpers
  - `supervisor_control.rs`, `supervisor_lookup.rs`, `supervisor_shutdown.rs`
    — supervisor surfaces for input, termination, lookup, and full shutdown
  - `lifecycle.rs` + `lifecycle/{monitor,shutdown,spawn}.rs` — spawn/monitor
    /shutdown internals
  - `tests/integration.rs` with `integration/` submodules for event flow,
    lifecycle, and fixture support (previously at `tests/process_manager_tests/**`)
- added `effigy-process` to the workspace and as a root-crate dependency
- deleted `src/process_manager.rs` and `src/process_manager/` entirely
- rewrote all 22+ import sites under `src/runner/**` and
  `src/tui/multiprocess/**` from `use crate::process_manager::*` to
  `use effigy_process::*` (no re-export retained — direct imports only)
- relocated integration tests from `tests/process_manager_tests/**` into
  `crates/effigy-process/tests/integration/**` and fixed the `#[path]`
  submodule bindings for the new layout
- converted `#[path = "process_manager/..."]` attributes in the moved lib
  into standard Rust module layout

## Churn Check

Real subsystem move. ~726 lines of supervision code now live in a dedicated
crate with its own 7-test integration harness (~297 lines of tests moved from
root to crate). Call sites updated in one coordinated sweep — no shim or
re-export residue.

## Vision Target Delta

- primary vision tags: `MAINT`, `CONTRACT`, `ROUTE`
- moved: process supervision ownership is now explicit and disjoint from
  container-execution routing (`effigy-exec`)
- remaining open: post-`232` boundary decision for the process subsystem

## Validation

- `cargo test -p effigy-process` — 7/7 integration tests green
- `cargo test` — full workspace green (11 test suites)
- `cargo fmt --all -- --check` — clean
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`233-decide-post-effigy-process-extraction-boundary.md`](../../../specs/batch-cards/233-decide-post-effigy-process-extraction-boundary.md)
to classify the remaining process-subsystem boundary honestly.
