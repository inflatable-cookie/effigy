# Effigy Demo Runner Runtime And Persistence Follow-up Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`165` widened `effigy-demo` with an active-state layer in
[crates/effigy-demo/src/active.rs](../../../crates/effigy-demo/src/active.rs).

That crate now owns:

- persisted active-attempt schema
- active-attempt file read/write/register/clear helpers
- active-attempt loading into `DemoActiveAttempt`
- terminal input and resize handoff file writes
- terminal handoff file preparation and cleanup
- recent-output loading from repo-relative paths

[src/runner/demo_command/mod.rs](../../../src/runner/demo_command/mod.rs) now uses thin adapter wrappers over that API instead of owning the active-state file contract inline.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `demo runner still owns active-state persistence and terminal handoff mechanics` -> current `effigy-demo owns active-state persistence and terminal handoff mechanics; runner is thinner`
- Remaining gap: `post-demo-runner boundary decision`

## Evidence

- `src/runner/demo_command.rs`: `4636` -> `4291` lines
- `crates/effigy-demo/src/active.rs`: new `439` line domain slice

## Validation Performed

- `cargo fmt --all`
- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`

## Next Task

Execute
`166-decide-post-demo-runner-runtime-and-persistence-follow-up-boundary.md`.
