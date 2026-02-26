# PATH Install and Release Workflow Validation

Date: 2026-02-26
Owner: Platform
Related roadmap: 001 - Effigy Foundation

## Scope
- Define and validate PATH-first execution workflow.
- Verify wrapper fallback behavior remains operational.
- Establish release checklist and smoke matrix baseline.

## Changes
- Added guide: `docs/guides/010-path-installation-and-release.md`.
- Added PATH install instructions (`cargo install --path . --root ...`).
- Added release checklist and smoke matrix for direct binary + wrapper modes.

## Validation
- command: `cargo test` (in `/Users/betterthanclay/Dev/projects/effigy`)
  - result: pass (21 tests).
- command: `cargo install --path . --root ./.local-install --force`
  - result: pass (`effigy` installed to `./.local-install/bin/effigy`).
- command: `./.local-install/bin/effigy --help`
  - result: pass (usage output shown).
- command: `PATH="/Users/betterthanclay/Dev/projects/effigy/.local-install/bin:$PATH" effigy pulse --repo .` (in `/Users/betterthanclay/Dev/projects/acowtancy`)
  - result: pass (pulse report rendered).
- command: `bun effigy tasks` (in `/Users/betterthanclay/Dev/projects/acowtancy`)
  - result: pass (catalogs/tasks listed through cargo-run wrapper).

## Risks / Follow-ups
- Wrapper mode can still wait on cargo cache/build locks under concurrent runs.
- PATH-first mode avoids cargo-run lock contention and should be preferred for daily use.

## Next
- Add phase `002` roadmap for binary distribution approach (tagged releases, install channels, and changelog cadence).
