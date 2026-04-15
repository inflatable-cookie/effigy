# Effigy Docs-Policy QA Check Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`134` is complete.

Effigy now has the reusable docs QA check cluster under
[`effigy-docs-policy`](../../../crates/effigy-docs-policy/Cargo.toml). The
remaining docs-policy logic no longer lives only in
[`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs).

## What Changed

- widened
  [`crates/effigy-docs-policy/src/lib.rs`](../../../crates/effigy-docs-policy/src/lib.rs)
  around reusable docs QA checks
- moved the shared docs QA cluster there:
  - link scanning
  - heading checks
  - contains checks
  - path checks
  - forbidden-text checks
  - workflow-path validation
- reconnected
  [`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs) so the
  docs command now adapts those checks instead of owning their logic inline

## Why A Boundary Decision Is Next

The remaining docs shell is smaller, but it is not yet obvious whether another
`effigy-docs-policy` extraction is still the right move.

What remains is more shell-shaped:

- docs command dispatch plus text/json rendering in
  [`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs)
- JSON-examples validation that may or may not earn another reusable slice

That needs an explicit boundary decision instead of another guessed batch.

## Current State

- active strict lane: `g02.010`
- active ready card: `135`
- queued release card: `115`

## Validation

- `cargo test -p effigy-docs-policy`
- `cargo test docs_command --lib`
- `cargo test --test cli_output_tests docs`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned docs QA checks`
  to `workspace-owned docs QA check surface with runner adapter wiring`
- remains open:
  - post-docs QA boundary classification for the remaining shell
  - later doctor and env / varlock modularization decisions
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`135-decide-post-docs-policy-qa-check-extraction-boundary.md`](../../specs/batch-cards/135-decide-post-docs-policy-qa-check-extraction-boundary.md)
to classify the remaining docs shell before modularization jumps to the next
domain cluster.
