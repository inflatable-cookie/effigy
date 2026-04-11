# Demo Registry And Inspection Foundation Implementation

Date: 2026-04-11
Roadmap: `g02.003`

## Summary

Shipped the first real `effigy demo` product surface.

This batch moved demo proof from planning-only doctrine into executable product
surface by adding manifest-backed demo registry loading, schema/doctor support,
`effigy demo list`, `effigy demo inspect <id>`, and normalized latest-attempt
inspection state based on declared receipts and artifacts.

## Delivered

- first-class `[demos.<id>]` manifest support in the runtime model
- doctor/schema acceptance for the demo registry
- `effigy demo list` in text and JSON modes
- `effigy demo inspect <id>` in text and JSON modes
- source provenance and normalized latest-attempt inspection
- user-facing docs for demo registry patterns and the new command surface

## Validation

- targeted parser/help/schema/demo CLI tests
- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Moved: `demo-harness planning-only contract -> shipped registry and inspection product surface`
- Remaining open: `demo run`, stop/rerun lifecycle work, and the later TUI/browser client

## Next Task

Use the next `g02.003` execution card to add `effigy demo run <id>` and
normalized attempt creation on top of this shipped inspection foundation.
