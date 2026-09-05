# Bun Full-Closure Planning

Status: complete
Created: 2026-08-05
Roadmap: `g08.021`
Batch: `1057`

## Summary

Established deterministic, non-mutating Bun link/unlink plans for the complete
matching package closure, with exact immutable-file snapshots and explicit
machine-registration ownership decisions.

## Changes

- inventoried root-only and workspace libraries and matched direct/transitive
  consumer packages as one indivisible closure
- modeled explicit `--no-save` registration, consumer-link, consumer-unlink,
  and registration-release intents
- snapshotted consumer manifests, Bun lockfiles, local package manifests,
  repo-local desired state, ignore state, and the machine registration index
- distinguished absent, matching foreign, Effigy-owned shared, stale, and
  conflicting global registrations without claiming foreign state
- planned reference-counted registration ownership and selected-consumer
  release through exact before/after index state
- proved planning and dry-run paths execute no mutating Bun process

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: Bun local linking had only a mechanism decision -> the full
  closure, immutable state, process intents, and registration ownership became
  deterministic typed plans
- Remaining gap: plan application, physical verification, unlink, and peer
  diagnostics remain in `1058` and `1059`

## Validation Performed

- `cargo test -p effigy-deps`
  - result: planning, inventory, ownership, and existing Cargo proofs passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: passed
- `effigy qa:ci:json`
  - result: passed
- `effigy qa:docs`
  - result: passed
- `cargo fmt --all -- --check`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- physical application still had to prove supported Bun save-less behavior and
  exact registration observations under the index lock

## Next Task

Continue the active Bun milestone through ready card `1059`.
