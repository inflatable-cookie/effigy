# Effigy Env Foundation Extraction

Date: 2026-04-16
Owner: Platform

## Summary

`136` is complete.

Effigy now has the reusable env-schema / varlock foundation under
[`effigy-env`](../../../crates/effigy-env/Cargo.toml). The remaining env-domain
logic no longer lives only in the root crate.

## What Changed

- added [`crates/effigy-env`](../../../crates/effigy-env/Cargo.toml)
- moved the env-domain module tree there:
  - schema parsing
  - resolution
  - validation
  - secret handling
- replaced [`src/env_schema.rs`](../../../src/env_schema.rs) with a
  compatibility shim over `effigy-env`
- reconnected [`src/runner/env_schema_support.rs`](../../../src/runner/env_schema_support.rs)
  and the task execution path so runtime env handling now adapts the extracted
  crate directly

## Why A Boundary Decision Is Next

The reusable env-domain surface is now real, but the remaining shell is smaller
and less obvious.

What remains is more runtime-shaped:

- runtime-specific schema enablement and `.env` loading policy in
  [`src/runner/env_schema_support.rs`](../../../src/runner/env_schema_support.rs)
- manifest integration choices that may or may not justify another extraction
  slice
- later vault-provider rollout work that belongs to `g02.009`, not this batch

That needs an explicit boundary decision instead of another guessed extraction.

## Current State

- active strict lane: `g02.010`
- active ready card: `137`
- queued release card: `115`

## Validation

- `cargo test -p effigy-env`
- `cargo test --test env_schema_tests`
- `cargo test --test cli_output_tests cli_catalog_task_json_mode_env_schema_sensitive_validation_redacts_error_message -- --nocapture`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `root-crate-owned env-schema / varlock module tree`
  to `workspace-owned effigy-env domain crate with runner adapter wiring`
- remains open:
  - post-env boundary classification for the remaining shell
  - later vault-backed rollout through `g02.009`
  - release closure and `v0.3` readiness through `g02.007` once the modularization bar is met

## Next Task

Execute
[`137-decide-post-env-foundation-extraction-boundary.md`](../../specs/batch-cards/137-decide-post-env-foundation-extraction-boundary.md)
to classify the remaining env / varlock shell before modularization jumps to
doctor, release, or another env slice.
