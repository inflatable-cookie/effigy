# Help Topic Duplicate Reduction

Date: 2026-05-20  
Roadmap: [`g07.068`](../../../roadmaps/g07/068-high-duplicate-help-fragment-reduction.md)  
Batch card: [`1018`](../../../roadmaps/g07/batch-cards/1018-trim-high-help-topic-duplicates.md)  
Strict lane: [`095`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- added two small help-topic DSL helpers in
  [`crates/effigy-cli/src/help/topics/shared.rs`](../../../../crates/effigy-cli/src/help/topics/shared.rs):
  - `text_lines!`
  - `option_rows!`
- rewrote the flagged help topics onto that tighter shape:
  - [`bootstrap.rs`](../../../../crates/effigy-cli/src/help/topics/bootstrap.rs)
  - [`container.rs`](../../../../crates/effigy-cli/src/help/topics/container.rs)
  - [`release.rs`](../../../../crates/effigy-cli/src/help/topics/release.rs)
- moved
  [`demo.rs`](../../../../crates/effigy-cli/src/help/topics/demo.rs)
  onto the shared `StandardTopicHelpSpec` path instead of hand-rendered sections
- added focused help tests in
  [`crates/effigy-cli/src/help/mod.rs`](../../../../crates/effigy-cli/src/help/mod.rs)
  for the `demo` and `release` common-option rows

## Proof

- `cargo fmt --all -- --check`: pass
- focused help proof:
  - `CARGO_TARGET_DIR=/tmp/effigy-cli-1018-target cargo test -p effigy-cli help::tests:: -- --nocapture`
- duplicate scan delta:
  - `effigy scan duplicate-blocks --json`
  - findings: `111 -> 108`
  - high findings: `7 -> 5`
  - remaining high findings are no longer in help topics

## Notes

This card stayed local to the help surface. No help wording or command grammar
was intentionally changed; the work only reduced repeated topic layout and moved
`demo` onto the same rendering contract already used by the other standard
topics.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `OPERATE`
- moved in this report: high duplicate help-topic findings `2 -> 0`; duplicate
  scan total `111 -> 108`
- remains open:
  - `1019`: remaining high language-emitter duplicate clusters
  - `1020`: remaining high runner-private fixture/helper duplicate cluster
  - `1021`: residual maintainability closeout
