# 682 - Normalize CLI Help Topic Data Shape

Roadmap: [`../038-docs-policy-cli-help-and-test-fixture-deduplication.md`](../038-docs-policy-cli-help-and-test-fixture-deduplication.md)
Strict lane: [`../../../specs/074-low-risk-deduplication-strict-lane.md`](../../../specs/074-low-risk-deduplication-strict-lane.md)
Contract: [`../../../contracts/030-low-risk-deduplication-contract.md`](../../../contracts/030-low-risk-deduplication-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Reduce repeated CLI help topic section construction where it can be normalized
without hiding help content.

## Scope

- inspect repeated help-topic blocks from the duplicate scan
- identify shared data shape candidates
- prefer readable helpers over macros
- keep rendered help text stable

## Non-Goals

- no public help redesign
- no command grammar changes
- no generated help snapshots unless already required by existing tests

## Acceptance

- repeated help-topic construction is reduced or explicitly deferred
- help tests still pass
- duplicate-block scan shows fewer high findings or records why help text
  repetition is intentionally accepted

## Outcome

- added a shared common option-row helper for repeated `--repo`, `--json`,
  `--help`, and release control option rows
- kept help topic usage, option, and example lists explicit in source review
- deferred broader help-topic normalization because the remaining high findings
  are scanner artifacts from literal-heavy arrays with different user-facing
  content

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy-cli`
- `cargo check --bin effigy`
- `effigy scan duplicate-blocks --json`
- `git diff --check`

## Next Task

Execute `683` to add private fixture builders where safe.
