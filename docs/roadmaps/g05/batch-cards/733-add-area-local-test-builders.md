# 733 - Add Area-Local Test Builders

Roadmap: [`../014-area-local-test-builder-cleanup.md`](../014-area-local-test-builder-cleanup.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)
Contract: [`../../../contracts/030-low-risk-deduplication-contract.md`](../../../contracts/030-low-risk-deduplication-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Reduce the highest-value remaining fixture duplication with private, local test
builders.

## Completed

- Added private release-fixture helpers in `tests/cli_output_tests/support.rs`.
- Switched the repeated CLI output release fixture writers over to the shared
  local helpers.
- Kept the cleanup local to the CLI output test area instead of adding a global
  test harness.

## Validation Notes

- Focused CLI output release tests pass.
- The broader duplicate scan still shows the same high findings because the
  remaining high-duplication seams are in bootstrap/release cross-file setup and
  literal-heavy help topics outside this local fixture slice.

## Next Task

Execute `734`.
