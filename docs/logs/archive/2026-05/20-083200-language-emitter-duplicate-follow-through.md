# Language Emitter Duplicate Follow Through

Date: 2026-05-20  
Roadmap: [`g07.069`](../../../roadmaps/g07/069-language-emitter-follow-through.md)  
Batch card: [`1019`](../../../roadmaps/g07/batch-cards/1019-follow-through-language-emitter-duplicates.md)  
Strict lane: [`095`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- added shared emitter helpers in
  [`crates/effigy-codegraph/src/language/emit.rs`](../../../../crates/effigy-codegraph/src/language/emit.rs)
  for:
  - parse-diagnostic dedupe and emission
  - owned symbol declaration and containment emission
  - scoped owned-symbol walks
- rewired the JS, PHP, and Python emitters onto those owners:
  - [`javascript.rs`](../../../../crates/effigy-codegraph/src/language/javascript.rs)
  - [`php.rs`](../../../../crates/effigy-codegraph/src/language/php.rs)
  - [`python.rs`](../../../../crates/effigy-codegraph/src/language/python.rs)
- removed the repeated local symbol/containment wrappers that were only
  forwarding into the shared emit path

## Proof

- `cargo fmt --all -- --check`: pass
- focused codegraph proof:
  - `CARGO_TARGET_DIR=/tmp/effigy-codegraph-1019-target cargo test -p effigy-codegraph --quiet`
  - `46` tests passed
- duplicate scan delta:
  - `effigy scan duplicate-blocks --json`
  - findings: `108 -> 103`
  - high findings: `5 -> 1`
  - remaining high finding moved out of the emitters and into the runner-private
    temp-repo helper pair targeted by `1020`

## Notes

This stayed local to the language-emitter surface. The extracted helpers own
only the repeated mechanics that were already semantically identical. Node-kind
selection, provenance, ranges, IDs, and child traversal still live at the call
sites.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `OPERATE`
- moved in this report: high duplicate emitter findings `4 -> 0`; duplicate
  scan total `108 -> 103`
- remains open:
  - `1020`: runner-private fixture/helper convergence
  - `1021`: residual maintainability closeout
