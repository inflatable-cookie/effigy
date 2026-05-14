# 731 - Extract Rhai Streaming Search And HTTP Support

Roadmap: [`../012-rhai-internal-boundary-follow-through.md`](../012-rhai-internal-boundary-follow-through.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Finish the next Rhai internal split by moving streaming, search, and HTTP support
out of `lib.rs`.

## Completed

- Added `crates/effigy-rhai/src/network_support.rs` for Rhai search and HTTP
  helper ownership.
- Rewired the Rhai host API to use the dedicated support module directly.
- Reduced `effigy-rhai/src/lib.rs` again by removing the moved search and HTTP
  implementation bodies.

## Validation Notes

- Focused Rhai search, HTTP, and streaming helper tests pass.
- The broader crate suite still carries the same pre-existing first-party script
  policy failures recorded in the lane state.

## Next Task

Execute `732`.
