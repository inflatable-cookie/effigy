# 093 Implement Keepsake Rhai Pilot Release Orchestration

Status: archived
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Widen the Rhai scripting surface into its first external Rust-first repo by
migrating one honest Keepsake orchestration script without jumping straight
into the heavier REAPER smoke wrappers.

## In Scope

- migrate Keepsake's `release:candidate:alpha` task off
  `tools/release-candidate.sh`
- replace it with a file-backed Rhai script referenced from the manifest
- keep the pilot bounded around artifact staging/packaging orchestration
- note any host-API gaps that show up in a real external repo

## Out Of Scope

- REAPER smoke wrapper migration
- Jetstream migration
- broad Keepsake scripting cleanup
- new Effigy scripting-policy replanning

## Acceptance Criteria

- Keepsake ships one real Rhai-backed operator task
- the old shell wrapper for that task is removed if the migration is clean
- the batch records whether the current host API is sufficient for a larger
  Keepsake follow-up
- the lane leaves one new explicit ready card

## Validation

- `cargo run --bin effigy -- tasks --repo ~/Dev/projects/keepsake`
- `cargo run --bin effigy -- release:candidate:alpha --repo ~/Dev/projects/keepsake`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Keepsake is temporarily deferred while parallel Windows-VM work is active there.
Return to this card only after the external-repo boundary is safe again or a
later decision explicitly revives the pilot.
