# 732 - Converge CLI Help Topic Descriptors

Roadmap: [`../013-cli-help-topic-descriptor-convergence.md`](../013-cli-help-topic-descriptor-convergence.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/030-low-risk-deduplication-contract.md`](../../../contracts/030-low-risk-deduplication-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Add one typed descriptor surface for CLI help topic registration and general
help inventory.

## Completed

- Added `crates/effigy-cli/src/help/registry.rs` as the shared descriptor owner.
- Moved builtin help topic lookup onto the shared descriptor surface.
- Moved general-help command inventory onto the same descriptor surface while
  keeping non-topic command rows explicit in `general.rs`.
- Switched help rendering dispatch through the registry without redesigning
  topic body text or introducing macros.

## Validation Notes

- Focused help dispatch and CLI output tests pass.
- `cargo test -p effigy-cli` still fails on the unrelated header-width unit test
  outside this card's seam.

## Next Task

Execute `733`.
