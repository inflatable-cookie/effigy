# Post-Docs-Policy QA Boundary Decision

Date: 2026-04-15
Owner: Platform

## Summary

`135` is complete.

The docs-policy extraction is now strong enough to move on. What remains in
[`src/runner/docs_command.rs`](../../../../src/runner/docs_command.rs) is mostly
command dispatch, rendering, and one smaller JSON-examples check surface, not
the next biggest reusable product cluster.

## Decision

Treat the remaining docs shell as honest adapter and local command policy work
for now:

- docs command dispatch plus text/json rendering
- JSON-examples validation in
  [`src/runner/docs_command.rs`](../../../../src/runner/docs_command.rs)

Do not force another `effigy-docs-policy` extraction batch yet.

## Why Env Is Next

The next clearly reusable cluster is env-schema / varlock:

- [`src/env_schema.rs`](../../../../src/env_schema.rs) still anchors a full
  product surface in the root crate
- that surface already has its own domain shape: parsing, resolution,
  validation, secret handling
- unlike the remaining docs shell, it still lacks a workspace crate boundary

That makes the next ready batch the first dedicated env extraction, not
another guessed docs slice.

## Current State

- active strict lane: `g02.010`
- active ready card: `136`
- queued release card: `115`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `ambiguous post-docs QA extraction target`
  to `explicit docs pause boundary plus env-schema / varlock as next modularization seam`
- remains open:
  - first `effigy-env` extraction
  - later doctor modularization if it still earns it
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`136-implement-effigy-env-foundation-extraction.md`](../../../specs/batch-cards/136-implement-effigy-env-foundation-extraction.md)
to move the env-schema / varlock foundation into its own workspace crate.
