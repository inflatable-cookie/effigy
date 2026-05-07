# 219 Implement Effigy Contracts Foundation Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the first trustworthy contracts-domain slice out of
`src/runner/contracts_command.rs` so the contracts surface stops living
entirely in the root crate.

## In Scope

- classify the contracts surface around:
  - JSON contract index loading
  - selection payload validation
  - selected-schema check orchestration
  - contracts command payload/result shaping
- create a dedicated workspace crate if the seam earns it
- reduce `src/runner/contracts_command.rs` materially
- leave only final runner-shell dispatch and output/error adaptation local

## Out Of Scope

- release execution
- demo/docs/container cleanup
- broad roadmap churn outside the active lane

## Acceptance Criteria

- the contracts surface no longer lives entirely in `src/runner/contracts_command.rs`
- the extracted contracts-domain logic sits behind a real workspace boundary
- the remaining runner file is mostly CLI entry, final rendering choice, and
  error mapping
- the next move is a boundary decision, not another guessed contracts slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`220-decide-post-contracts-foundation-extraction-boundary.md`](./220-decide-post-contracts-foundation-extraction-boundary.md)
to decide whether the contracts seam can pause or still needs one more bounded
follow-up.
