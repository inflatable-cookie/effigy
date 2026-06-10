# 2026-04-14 22:35:00 Post-Effigy Rhai Dogfooding Slice Decision

## Decision

The next Rhai slice is a bounded host-API expansion for signal-aware
long-running scripts.

## Why

The first substantial Effigy dogfooding cluster proved the current file-backed
Rhai surface is already good enough for:

- local install/link helpers
- structured release smoke checks
- short-lived proof/report demos

It also exposed the main remaining gap honestly:

- `lifecycle-window` still cannot migrate cleanly because Rhai lacks a bounded,
  explicit way to respond to stop/termination and run cleanup/status writes for
  long-running scripts

That is a more meaningful next slice than:

- more Effigy dogfooding on smaller short-lived wrappers
- starting Keepsake before the long-running-script gap is solved

## Consequence

- Effigy remains the active dogfooding repo
- Keepsake stays deferred until after the long-running lifecycle boundary lands
- Jetstream remains out of scope for now

## Next Task

Implement the bounded long-running Rhai lifecycle slice next:

- detect stop intent / termination inside Rhai
- support cleanup/status writes on shutdown without arbitrary shell emulation
- migrate `lifecycle-window` only if that support lands cleanly
