# 2026-04-17 03:45:00 BST — Post Effigy Process Extraction Boundary Decision

## Summary

Process supervision pauses cleanly.

After `232`, no `process_manager` references remain anywhere in the root
crate. All 115 references to process-supervision types live with
`use effigy_process::*` — the crate is imported directly at the call site,
with no wrapper, adapter, or shim residue in the root crate.

`src/process_manager.rs` and `src/process_manager/` are gone. Integration
tests moved with the code. The subsystem is now a standalone cross-cutting
crate.

## Why This Decision

Further extraction would push genuine root-crate wiring (runner error mapping,
TUI multiprocess event plumbing, demo/container ProcessSpec construction)
into the crate. That breaks the boundary in the wrong direction — the crate
should own supervision mechanics, not application-specific orchestration.

The crate already owns:

- process specs and events
- supervisor lifecycle (spawn, monitor, shutdown, restart)
- stdio streaming + signal handling
- exit diagnostics
- its own integration test harness (7 tests, ~297 lines)

That is the right scope for a standalone subsystem.

## Decision

- pause process supervision on the current boundary
- keep `effigy-process` as the owner of `ProcessSpec`, `ProcessEvent`,
  `ProcessSupervisor`, and all lifecycle machinery
- move the active lane to the next `g02.017` queue job

## Churn Check

Real boundary. The root crate shed ~726 lines of subsystem code in `232`; the
remaining `use effigy_process::*` imports across 30 files are the minimum
viable caller surface for a cross-cutting subsystem.

## Vision Target Delta

- primary vision tags: `MAINT`, `CONTRACT`, `ROUTE`
- moved: process supervision now paused on a clean cross-cutting crate
  boundary, disjoint from `effigy-exec` container-routing
- remaining open: pick the next `g02.017` queue job and execute it

## Validation

- `cargo test` — full workspace green (11 suites)
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`234-decide-next-src-shell-cleanup-priority-after-effigy-process-pause-boundary.md`](../../specs/batch-cards/234-decide-next-src-shell-cleanup-priority-after-effigy-process-pause-boundary.md)
to pick the next `/src` cleanup priority after pausing process supervision.
