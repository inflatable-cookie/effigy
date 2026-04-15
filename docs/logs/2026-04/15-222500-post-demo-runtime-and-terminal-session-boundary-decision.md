# Post-Demo Runtime And Terminal Session Boundary Decision

Date: 2026-04-15
Owner: Platform

## Summary

`131` is complete.

The remaining demo shell is no longer the best modularization target. The
runtime/session extraction moved enough shared demo ownership into
[`effigy-demo`](../../../crates/effigy-demo/Cargo.toml) that what remains is
mostly runner and TUI adapter behavior.

## Decision

Treat the remaining demo surface as honest shell/TUI work for now:

- launch, stop, rerun, and concurrent-runner orchestration in
  [`src/runner/demo_command.rs`](../../../src/runner/demo_command.rs)
- browser-local live terminal session driving and rendering behavior in
  [`src/tui/demo_browser.rs`](../../../src/tui/demo_browser.rs)

Do not force another `effigy-demo` extraction batch yet.

## Why Docs-Policy Is Next

The next clearly reusable cluster is docs-policy:

- [`src/runner/docs_command.rs`](../../../src/runner/docs_command.rs) is still a
  large runner-owned surface
- the docs QA/index/next-action checks are product logic reused by the repo’s
  own planning and release discipline
- unlike the remaining demo shell, that surface still has an obvious first
  foundation slice that should live behind a workspace crate

That makes the next ready batch the first dedicated docs-policy extraction,
not another guessed demo slice.

## Current State

- active strict lane: `g02.010`
- active ready card: `132`
- queued release card: `115`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `ambiguous post-demo extraction target`
  to `explicit demo pause boundary plus docs-policy as next modularization seam`
- remains open:
  - first `effigy-docs-policy` extraction
  - later env / varlock and any still-justified doctor modularization
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`132-implement-effigy-docs-policy-foundation-extraction.md`](../../specs/batch-cards/132-implement-effigy-docs-policy-foundation-extraction.md)
to move the next clearly reusable docs-policy surface out of
`src/runner/docs_command.rs`.
