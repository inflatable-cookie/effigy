# 131 Decide Post-Demo Runtime And Terminal Session Extraction Boundary

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining demo-owned shell still justifies another
`effigy-demo` extraction batch or whether the demo domain is now clean enough
for modularization to move to the next largest interleaved cluster.

## In Scope

- assess the remaining demo weight across `src/runner/demo_command.rs` and
  `src/tui/demo_browser.rs`
- distinguish honest shell/TUI adapter work from still-reusable demo-domain
  logic
- leave the next ready batch explicit

## Out Of Scope

- implementing another extraction slice in the same batch
- release closure
- env or docs-policy extraction unless the decision explicitly promotes one of
  them next

## Acceptance Criteria

- the remaining demo shell is classified honestly
- the next modularization move is explicit
- `g02.010` currentness stays trustworthy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`132-implement-effigy-docs-policy-foundation-extraction.md`](./132-implement-effigy-docs-policy-foundation-extraction.md)
to move the next clearly reusable docs-policy surface out of
`src/runner/docs_command.rs`.
