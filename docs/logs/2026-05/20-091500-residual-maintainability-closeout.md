# Residual Maintainability Closeout

Date: 2026-05-20  
Roadmap: [`g07.071`](../../roadmaps/g07/071-residual-maintainability-closeout.md)  
Batch card: [`1021`](../../roadmaps/g07/batch-cards/1021-close-residual-maintainability-lane.md)  
Strict lane: [`095`](../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- closed the reopened residual-maintainability tranche after:
  - splitting manifest semantic ownership
  - decomposing the codegraph crate test harness
  - pulling Rhai script-command dispatch out of the runner glue owner
  - reducing the last high help-topic duplicate clusters
  - reducing the last high language-emitter duplicate clusters
  - removing the final high runner-private helper duplicate
- kept the lane bounded to maintainability work; no public CLI, JSON, release,
  or workflow behavior was intentionally changed

## Scan Delta

Baseline from `1014`:

- `effigy scan god-files --json`: `3` findings, all `warning`
- `effigy scan duplicate-blocks --json`: `111` findings, `7 high`

Closeout rerun:

- `effigy scan god-files --json`: `0` findings
- `effigy scan duplicate-blocks --json`: `103` findings, `0 high`, `0 critical`

Interpretation:

- the lane removed the last warning-only oversized ownership files rather than
  just holding the line
- the lane also cleared every high duplicate-block finding without turning the
  repo into a generic helper maze
- warning-level duplicate blocks remain, but none justify more cleanup without
  a new bounded case

## Focused Validation

- `cargo fmt --all -- --check`
- `CARGO_TARGET_DIR=/tmp/effigy-codegraph-1019-target cargo test -p effigy-codegraph --quiet`
- `CARGO_TARGET_DIR=/tmp/effigy-1020-target cargo test -p effigy --lib non_primary_service_exec_does_not_force_primary_working_dir -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-1020-target cargo test -p effigy --lib run_container_eject_promotes_generated_compose -- --nocapture`

## Broad QA

- `CARGO_TARGET_DIR=/tmp/effigy-1021-target effigy qa`
  - `1518` tests passed, `1` skipped
  - docs link, JSON-example, index, forbidden-text, heading, workflow-path,
    next-action, and vision-policy checks passed
  - fast JSON contract checks passed

## Remaining Debt

Follow-up candidates:

- warning-only duplicate blocks in help topics, catalog fixtures, and a few
  language-emitter/test-support surfaces if a future lane can prove a clearer
  owner than the current local repetition

Defer:

- further duplicate-count headline reduction that only moves repeated literals
  behind helpers

Not worth doing now:

- chasing zero warning duplicates or forcing every test policy constructor into
  shared helpers; the remaining blocks are not carrying the same maintenance
  cost as the ones this lane removed

## Vision Target Delta

- primary vision tags touched: `MAINT`, `OPERATE`
- moved:
  - residual god-file findings `3 -> 0`
  - high duplicate-block findings `7 -> 0`
  - the reopened `g07` tranche now closes on broad QA with no live ready card
- remains open:
  - no active execution lane; next work starts with planning

## Next Task

No active ready card.
