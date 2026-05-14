# 730 - Extract Rhai Internal Secrets And Process Support

Roadmap: [`../012-rhai-internal-boundary-follow-through.md`](../012-rhai-internal-boundary-follow-through.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Move the first major internal seams out of `effigy-rhai/src/lib.rs`: secret
store ownership and process execution support, while aligning the crate with the
shared vault-access posture introduced in the runner.

## Completed

- Added `crates/effigy-rhai/src/rhai_secrets.rs` for Rhai secret-store and vault
  mutation ownership.
- Added `crates/effigy-rhai/src/process_support.rs` for process execution and
  streaming helper ownership.
- Replaced the large `lib.rs` implementations with thin delegating wrappers so
  host API surfaces stay stable while the real logic moves out.

## Validation Notes

- Focused Rhai secret and process helper tests pass.
- `cargo test -p effigy-rhai` still fails on two pre-existing first-party script
  policy checks outside this card's seam.

## Next Task

Execute `731`.
