# 013 Implement Demo Registry And Inspection Foundation

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Implement the first demo-runner foundation slice in Effigy.

## In Scope

- add manifest-backed demo registry loading from `[demos.<id>]`
- add the minimum schema and doctor support for demo declarations
- add `effigy demo list` in text and JSON modes
- add `effigy demo inspect <id>` in text and JSON modes
- normalize latest-attempt receipt/artifact state enough for inspection

## Out Of Scope

- `effigy demo run`
- `effigy demo stop`
- `effigy demo rerun`
- TUI/browser implementation
- consumer-repo migration work

## Acceptance Criteria

- Effigy can load declared demos from the manifest model
- operators can list demos without digging through raw config
- operators can inspect one demo's metadata, coverage, and latest known proof
  state
- the implementation leaves runner execution for a later bounded batch

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch starts adding run/stop/rerun behavior
- the batch drifts into TUI implementation
- the normalized inspection state becomes coupled to Signal-specific files

## Next Task

Execute the next bounded runner card for demo run semantics and normalized
attempt creation.
