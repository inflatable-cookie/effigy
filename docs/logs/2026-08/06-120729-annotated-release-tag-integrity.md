# Annotated Release Tag Integrity

Status: complete
Created: 2026-08-06
Roadmap: g08.025
Batch: annotated-release-tag-integrity

## Summary

- Promoted Contract 035: every Effigy-created release tag is annotated and its
  message exactly equals the rendered tag.
- Replaced lightweight tag creation with option-safe annotated creation.
- Added direct Git-object proof and extended the complete release-execute
  fixture across its local and bare-remote repositories.
- Published the exact operator rule in the release guide and changelog.
- Reinstalled the repaired local binary and reran Swallowtail's full read-only
  source-candidate simulation.

## Evidence

The direct primitive test proves:

- object type `tag`
- annotation message `v0.1.0`
- peeled identity equal to the intended commit

The execute-success fixture proves the local and pushed bare-remote
`release-0.2.5` refs are both tag objects, both carry message
`release-0.2.5`, and both peel to the release commit returned by Effigy.

Swallowtail candidate
`0ef25a8c4f8bb9ee5c7c71b27cb0c4df0f608b01` remains ready:

- 11 of 11 release gates pass
- 1,463 tests pass; 11 are skipped
- the isolated source consumer resolves the exact candidate revision
- simulation writes no prepared state and performs no release mutation

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`
- Movement: baseline `release execute created a lightweight ref despite
  consumer contracts approving an annotated tag` -> current `release execute
  creates and pushes one deterministic annotated tag object`
- Remaining gap: None for tag object identity. Actual release authorization
  remains consumer- and operator-owned.

## Validation Performed

- `cargo test -p effigy-release`
  - result: 14 passed
- `cargo test --test cli_output_tests cli_release_execute_ -- --nocapture`
  - result: 18 passed
- `cargo clippy -p effigy-release -p effigy --all-targets -- -D warnings`
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `effigy qa:docs`
  - result: pass
- Swallowtail `effigy --json release simulate`
  - result: ready; 11 of 11 gates pass
- `git diff --check`
  - result: pass

## Boundaries

No real release prepare, execute, tag, branch push, tag push, registry
publication, workflow edit, GitHub Release, or authenticated provider work ran.

## Next Task

Select the next substantial g08 scope separately. No release or generation
rollover is implied.
