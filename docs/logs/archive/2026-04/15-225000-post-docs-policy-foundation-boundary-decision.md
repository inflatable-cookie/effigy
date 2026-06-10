# Post-Docs-Policy Foundation Boundary Decision

Date: 2026-04-15
Owner: Platform

## Summary

`133` is complete.

The first docs-policy slice was meaningful, but it did not reduce
[`src/runner/docs_command.rs`](../../../../src/runner/docs_command.rs) to mostly
adapter work. One more docs-policy extraction is still justified before the
lane jumps to another domain.

## Decision

Do not pause docs-policy yet.

The remaining docs shell still contains a reusable QA check cluster:

- link scanning
- heading/content/path checks
- workflow-path validation

That means the next honest move is another `effigy-docs-policy` extraction
batch, not a doctor or env jump.

## Current State

- active strict lane: `g02.010`
- active ready card: `134`
- queued release card: `115`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `ambiguous post-docs extraction target`
  to `explicit second docs-policy slice before cross-domain modularization continues`
- remains open:
  - docs-policy QA check extraction
  - later doctor and env / varlock modularization decisions
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`134-implement-effigy-docs-policy-qa-check-extraction.md`](../../../specs/batch-cards/134-implement-effigy-docs-policy-qa-check-extraction.md)
to move the remaining docs QA check cluster out of
`src/runner/docs_command.rs`.
