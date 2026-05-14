# g05.013 - CLI Help Topic Descriptor Convergence

Status: Complete
Depends on: `g05.008`
Contract: [`030-low-risk-deduplication-contract.md`](../../contracts/030-low-risk-deduplication-contract.md)

## Goal

Collapse CLI help topic registration to one source of truth while keeping topic
content explicit and reviewable in source.

## Evidence

- `crates/effigy-cli/src/lib.rs`, `command_parsing.rs`, `help/mod.rs`, and
  `help/topics/general.rs` all track the help topic set separately
- duplicate-block scan still reports high findings across help topic files
- the current dedup contract already prefers readable data normalization over
  macros or generated code

## Scope

- add one typed topic descriptor surface for registration and general-help
  inventory
- derive parser lookup and render dispatch from that source where practical
- keep topic body text in current topic modules
- reduce review churn when built-in help surfaces change

## Non-Goals

- no help copy redesign
- no command grammar change
- no macro-heavy code generation

## Acceptance Criteria

- topic registration lives in one clear owner
- adding or renaming a help topic requires one primary registration edit
- help output remains stable and source review stays readable

## Suggested Validation

- `cargo test -p effigy-cli`
- targeted help/parse tests
- `effigy scan duplicate-blocks --json`

## Next Task

Open a card for the descriptor-table introduction and the first parser/render
registration convergence pass.
