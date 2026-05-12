# 681 - Consolidate Docs Policy Test Ownership

Roadmap: [`../038-docs-policy-cli-help-and-test-fixture-deduplication.md`](../038-docs-policy-cli-help-and-test-fixture-deduplication.md)
Strict lane: [`../../../specs/074-low-risk-deduplication-strict-lane.md`](../../../specs/074-low-risk-deduplication-strict-lane.md)
Contract: [`../../../contracts/030-low-risk-deduplication-contract.md`](../../../contracts/030-low-risk-deduplication-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Remove duplicated docs-policy domain behavior tests from the docs command runner
test module.

## Scope

- keep domain behavior tests in `crates/effigy-docs-policy/src/tests.rs`
- keep runner tests focused on runner-owned error/report adaptation
- remove direct docs-policy helper imports from runner tests unless they prove
  runner behavior
- confirm duplicate-block critical findings for this pair are gone

## Non-Goals

- no docs policy behavior changes
- no docs command grammar changes
- no JSON contract changes
- no help text changes

## Acceptance

- `effigy-docs-policy` tests still pass
- docs command runner tests still pass
- duplicate-block scan no longer reports critical duplicates for the
  docs-policy test pair

## Outcome

- removed duplicated docs-policy domain behavior tests from
  `src/runner/docs_command/tests.rs`
- kept runner coverage focused on docs-policy error mapping
- reduced duplicate-block scan from `critical=2` to `critical=0`

## Validation

- `cargo test -p effigy-docs-policy`
- `cargo test docs_command`
- `cargo check --bin effigy`
- `effigy scan duplicate-blocks --json`
- `git diff --check`

## Next Task

Execute `682` to normalize CLI help topic data shape.
