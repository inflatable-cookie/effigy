# 2026-05-14 16:50:09 - Low-Risk Deduplication Follow-Through

Roadmap: [`g05.020`](../roadmaps/g05/020-reusable-core-hardening-suite.md)  
Batch card: [`746`](../roadmaps/g05/batch-cards/746-reduce-high-duplicate-blocks.md)  
Strict lane: [`083`](../specs/083-reusable-core-hardening-strict-lane.md)

## What Changed

- moved shared bootstrap root fixture builders into
  `crates/effigy-bootstrap/tests/support.rs`
- reused those bootstrap helpers from
  `crates/effigy-bootstrap/tests/integration.rs` and
  `src/runner/bootstrap_command/tests.rs`
- added `crates/effigy-release/src/test_support.rs` for repeated version-file
  assertion helpers
- reused the same release assertion helpers from
  `crates/effigy-release/src/tests.rs` and
  `src/runner/release_command/tests.rs`

## Measured Result

- duplicate-block scan moved from `findings=98 high=6?` baseline before this
  slice? No: this slice started from `findings=98 high=8` after the earlier
  reusable-core audit sequence and now reports `findings=94 high=6`
- bootstrap and release duplicate proof blocks are no longer reported as high
  findings

## Intentionally Retained

- high duplicate help-topic arrays in `crates/effigy-cli/src/help/topics/*`
- container temp-repo helper duplication in runner container tests/support

These remain to avoid forcing broad abstractions with weak payoff.

## Validation

- `cargo test -p effigy-bootstrap`
- `cargo test release`
- `effigy scan duplicate-blocks --json`
- `git diff --check`
