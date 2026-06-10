# Effigy Demo Record And Projection Follow-up Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`167` moves the shared demo record/projection layer into
[crates/effigy-demo/src/records.rs](../../../../crates/effigy-demo/src/records.rs).

[src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs)
no longer owns `DemoRecord`, `DemoActionAvailability`, `DemoGroup`,
`DemoEntrypoint`, or the shared history/grouping projection helpers directly.
The runner now adapts those crate-owned contracts through thinner query and
text-render helpers.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `demo runner still owns shared record/projection layer` ->
  current `demo record/projection ownership moved behind effigy-demo`
- Remaining gap: `demo_command runner shell still mixes render/orchestration/runtime
  work and needs one more boundary decision`

## Evidence

- new shared crate module:
  [crates/effigy-demo/src/records.rs](../../../../crates/effigy-demo/src/records.rs)
- runner file reduction:
  [src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs)
  `4292 -> 3964` lines
- moved contracts:
  - `DemoRecord`
  - `DemoActionAvailability`
  - `DemoGroup`
  - `DemoEntrypoint`
  - history attempt projection helpers
  - group-building helpers

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`168-decide-post-demo-record-and-projection-follow-up-boundary.md`.
