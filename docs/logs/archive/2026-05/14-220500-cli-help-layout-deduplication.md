# CLI Help Layout Deduplication

Date: 2026-05-14
Roadmap: `g06.005`
Batch card: `805`

## Summary

Reduced the remaining high-value CLI help-topic duplication by moving the
largest topic render calls onto a shared spec-driven render path.

## Changes

- added `StandardTopicHelpSpec` plus shared spec rendering in
  [`crates/effigy-cli/src/help/topics/shared.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-cli/src/help/topics/shared.rs)
- converted these topics to spec-owned local content plus one shared render
  call:
  - [`bootstrap.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-cli/src/help/topics/bootstrap.rs)
  - [`docs.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-cli/src/help/topics/docs.rs)
  - [`container.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-cli/src/help/topics/container.rs)
  - [`release.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-cli/src/help/topics/release.rs)

## Outcome

- duplicate-block findings stayed at `93`
- high duplicate-block findings dropped from `6` to `4`
- the largest help-topic boilerplate cluster is reduced
- help topic copy still stays local to each owner file

## Validation

- `cargo test help`
- `cargo test tasks_rendering`
- `cargo run --bin effigy -- scan duplicate-blocks --json`
