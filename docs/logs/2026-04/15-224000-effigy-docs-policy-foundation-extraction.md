# Effigy Docs-Policy Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`132` is complete.

Effigy now has a real
[`effigy-docs-policy`](../../../crates/effigy-docs-policy/Cargo.toml)
workspace crate. The first shared docs-policy contracts no longer live only in
[`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs).

## What Changed

- added
  [`crates/effigy-docs-policy`](../../../crates/effigy-docs-policy/Cargo.toml)
- moved the first reusable docs-policy slice there:
  - docs index-policy resolution
  - next-action policy resolution
  - next-action allowlist loading
  - log-index path normalization and insertion
  - shared markdown section, fenced-json, lead-verb, and index-link helpers
- reconnected
  [`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs) so the
  docs command now adapts those contracts instead of owning them inline

## Why A Boundary Decision Is Next

The remaining docs shell is smaller, but it is not yet obvious whether another
`effigy-docs-policy` extraction is still the right move.

What remains is more shell-shaped:

- command dispatch plus text/json rendering for docs checks in
  [`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs)
- link scanning and workflow-path checks that may or may not earn another
  reusable slice

That needs an explicit boundary decision instead of another guessed batch.

## Current State

- active strict lane: `g02.010`
- active ready card: `133`
- queued release card: `115`

## Validation

- `cargo test -p effigy-docs-policy`
- `cargo test docs_command --lib`
- `cargo test --test cli_output_tests docs`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned docs index-policy and next-action logic`
  to `workspace-owned docs-policy foundation with runner adapter wiring`
- remains open:
  - post-docs boundary classification for the remaining shell
  - later env / varlock and any still-justified doctor modularization
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`133-decide-post-docs-policy-foundation-extraction-boundary.md`](../../specs/batch-cards/133-decide-post-docs-policy-foundation-extraction-boundary.md)
to classify the remaining docs shell before modularization jumps to the next
domain cluster.
