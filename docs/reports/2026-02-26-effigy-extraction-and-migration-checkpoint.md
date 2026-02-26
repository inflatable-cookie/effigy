# Effigy Extraction and Migration Checkpoint

Date: 2026-02-26
Owner: Platform
Related roadmap: 001 - Effigy Foundation

## Scope
- Capture extraction status from Underlay.
- Record initial consumer migration status.
- Confirm docs baseline creation.

## Changes
- Extracted runner into standalone `effigy` repository.
- Preserved legacy catalog compatibility fallback.
- Migrated initial consumer repos to `effigy` command surface.
- Removed embedded `underlay-cli` crate from Underlay.
- Added architecture/roadmap/reports docs skeleton in Effigy.

## Validation
- command: `cargo test` (effigy)
  - result: pass.
- command: `bun effigy tasks` (consumer roots)
  - result: pass for migrated repos.
- command: `cargo metadata --no-deps -q` (underlay)
  - result: pass after runner extraction.

## Risks / Follow-ups
- PATH-based direct invocation is not yet standardized.
- Wrapper invocations still rely on cargo build locks during active development.

## Next
- Add PATH-first install guidance and smoke validation.
- Define release/versioning workflow.
