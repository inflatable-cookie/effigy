# Cargo Full Closure Planning

Status: complete
Created: 2026-08-05
Roadmap: `g08.020`
Batch: `1054`

## Summary

Added a pure Cargo link/unlink planner with exact file deltas, full-closure
source grouping, tracked-config and dirty-lock guards, and safe-unlink ownership.

## Changes

- grouped direct/transitive matches by every exact declared Git source URL
- mapped workspace and workspace-less library crates to canonical local paths
- planned one managed repo-root Cargo block with exact config, ignore, ledger,
  affected-lock, and directory-cleanup evidence
- preserved foreign config and rejected tracked config, patch collisions,
  malformed markers, path/registry/unmatched closures, no-match, and dirty locks
- made re-link idempotent and unlink ownership-specific
- added a read-only Git observer using only `git ls-files` and porcelain status
- expanded desired package state to retain multiple committed source identities
- recorded Cargo config/directory ownership required for safe unlink cleanup
- prepared ready apply/verify card `1055` and planned closeout card `1056`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: Cargo inventory could describe resolution -> one deterministic plan
  now proves the complete local override delta without writing it
- Remaining gap: apply/local verification and unlink/remote recovery remain in
  cards `1055` and `1056`

## Validation Performed

- `cargo test -p effigy-deps`
  - result: 36 tests and doc tests passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: passed
- `cargo check -p effigy`
  - result: passed
- `effigy qa:ci:fast`
  - result: 1,618 tests passed, 1 skipped; compatibility and JSON checks passed
- `effigy scan god-files --path crates/effigy-deps/src/cargo_plan.rs --show-warnings`
  - result: no findings
- `effigy qa:docs`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- file application and Cargo verification remain deliberately unavailable until
  `1055`
- unlink lock re-resolution remains `1056`; the planner only identifies affected
  tracked locks and blocks pre-dirty link state

## Next Task

Execute ready batch card `1055`.
