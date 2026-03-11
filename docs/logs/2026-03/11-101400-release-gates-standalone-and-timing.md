# Release Gates Standalone And Timing

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-gates-standalone-and-timing

## Summary

- Added `effigy release gates` as the standalone release gate command.
- Release gate execution now records timing metadata and stops on the first
  failure instead of continuing through the full list.
- The same timed sequential runner now backs standalone and integrated release
  gate checks.

## Changes

- Added `release gates` to the CLI parser, help text, and release command
  dispatch.
- Refactored release gate execution into a timed sequential runner that records
  per-gate duration and total elapsed time.
- Added fail-fast behavior so later gates are not run after the first failure.
- Added standalone text and JSON output contracts for release gate runs,
  including blocker reporting and captured failed-gate output.
- Threaded timing metadata into existing release gate result payloads so
  `status` and `prepare` now report the same gate timing structure.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `Effigy had embedded gate checks but no standalone release gate surface or timing contract` -> current `Effigy can now run release gates directly with stable timing and fail-fast behavior across the release workflow`
- Remaining gap: `simulate flow, interactive approvals, self-hosting migration, and broader adoption docs remain open`

## Validation Performed

- command: `cargo test --lib parse_release_ -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_gates_ -- --nocapture`
  - result: pass

## Risks

- Standalone gate execution is intentionally fail-fast now, which is a good fit
  for operator workflows but means it does not collect a full matrix of all
  failing gates in one run.
- Timing is wall-clock based and intentionally simple; if later reporting needs
  more detailed breakdowns, the current duration fields are the starting point
  rather than the final analytics surface.

## Next Task

- Implement the next meaningful `g01.027` batch by adding `effigy release simulate`
  on top of the now-complete status/prepare/execute/gates building blocks, so
  operators can preview the full release flow without writing files, creating
  state, or touching git history.
