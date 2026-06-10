# Release Simulate Full Dry Run

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-simulate-full-dry-run

## Summary

- Added `effigy release simulate` as the safe full-preview release command.
- The command runs release gates, previews planned version/changelog mutations,
  and shows the commit/tag that would be created.
- The simulation is explicitly non-destructive: it does not rewrite files and
  does not create `.release-prepared.json`.

## Changes

- Added `release simulate` to the CLI parser, release help topic, and release
  command dispatch.
- Added a dedicated simulation payload and text/JSON renderers in the release
  runtime so operators get one contract for dry-run output instead of piecing
  together `status`, `gates`, and `prepare --plan`.
- Reused the existing release context, mutation planning, and sequential
  fail-fast gate runner so simulation stays aligned with real prepare behavior.
- Added CLI tests proving simulation reports planned mutations and commit/tag
  previews without touching the working tree or writing release state, and that
  gate failures stop later gates from running.
- Updated the release protocol guide, roadmap progress, and changelog entries
  to include the new command surface.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `PREVIEW`
- Movement: baseline `Effigy had status, gates, prepare, and execute surfaces but no single full dry-run command` -> current `Effigy now provides a first-class simulate command for complete safe release previewing`
- Remaining gap: `interactive prompts, self-hosting migration, cross-project validation, and broader release docs remain open`

## Validation Performed

- command: `cargo test --lib parse_release_ -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_simulate_ -- --nocapture`
  - result: pass

## Risks

- Simulation intentionally does not preflight branch/remote/push requirements;
  those remain part of the later execute-stage checks because they depend on a
  prepared state and live git context.
- Gate commands still execute for real during simulation, so projects must keep
  release gates read-only if they want a perfectly side-effect-free dry run.

## Next Task

- Implement the next meaningful `g01.027` batch by starting Effigy’s
  self-hosting migration: add `[release]` config to Effigy’s own
  `effigy.toml`, mirror the current release scripts as built-in gates, and add
  validation that the new release flow matches the existing script outputs
  before any workflow-level migration.
