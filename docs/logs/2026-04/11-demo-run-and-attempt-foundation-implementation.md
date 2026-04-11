# Demo Run And Attempt Foundation Implementation

Date: 2026-04-11
Roadmap: `g02.003`

## Summary

Shipped the first executable demo-attempt surface on top of the registry and
inspection foundation.

This batch moved `effigy demo` beyond passive discovery by adding
`effigy demo run <id>` for task-backed and run-backed demos, normalized receipt
writing, and immediate latest-attempt refresh so `demo inspect` reflects newly
executed proof instead of only previously declared artifacts.

## Delivered

- `effigy demo run <id>` in text and JSON modes
- execution support for task-backed and run-backed demo entrypoints
- normalized pass/fail latest-attempt creation
- default receipt writing to `.effigy/demo/receipts/<demo-id>.json` when the
  manifest does not declare `receipt`
- immediate latest-attempt refresh so `demo inspect` reflects the newly
  recorded proof run
- user-facing docs for demo execution on top of the registry foundation

## Validation

- targeted parser/help/demo CLI tests
- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`
- Moved: `demo registry and inspection only -> first-class demo run and normalized attempt creation`
- Remaining open: active-attempt lifecycle control, `demo stop`, `demo rerun`, and the later TUI/browser client

## Next Task

Use the next `g02.003` ready card to decide active-attempt, stop, and rerun
semantics before more lifecycle control is implemented.
