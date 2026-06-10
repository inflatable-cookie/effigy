# Codegraph Test Harness Decomposition

Date: 2026-05-19
Roadmap: [`g07.066`](../../../roadmaps/g07/066-codegraph-test-harness-decomposition.md)
Batch card: [`1016`](../../../roadmaps/g07/batch-cards/1016-decompose-codegraph-test-harness.md)
Strict lane: [`095`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- replaced the monolithic `crates/effigy-codegraph/src/tests.rs` with
  `crates/effigy-codegraph/src/tests/mod.rs`
- moved graph proof into owned test families:
  - `storage_contracts.rs`
  - `index_lifecycle.rs`
  - `context_quality.rs`
  - `manifest_semantics.rs`
  - `language_indexers.rs`
- kept the shared fixture writers and graph payload helpers local to the test
  module instead of pushing them into opaque support layers

## Proof

- `cargo fmt --all -- --check`
- `cargo test -p effigy-codegraph --quiet`
- `effigy scan god-files --json`

Results:

- `46` codegraph tests passed
- the codegraph test harness no longer appears in god-file findings
- residual god-file scan is now down to one warning-only file:
  `src/runner/script_command/mod.rs`

## Vision Target Delta

- primary tags: `MAINT`, `CONTRACT`
- moved: residual maintainability lane from `2` code warning-only god files to
  `1`, with the codegraph proof surface split by owner and failure locality
  improved
- remains open: `1017` script-command owner sprawl, plus later duplicate/help
  and runner-helper follow-through cards
