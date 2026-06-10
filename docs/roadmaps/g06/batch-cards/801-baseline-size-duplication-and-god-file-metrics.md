# 801 - Baseline Size, Duplication, And God-File Metrics

Roadmap: [`../001-codebase-lean-down-suite.md`](../001-codebase-lean-down-suite.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Capture the lean-down baseline before any deletion batches start.

## Scope

- record current Rust LOC
- record broader source/config surface LOC if useful
- record current duplicate-block findings
- record warning-level god files
- capture the initial ranked target list for `802` through `808`

## Guardrails

- no code movement in this card
- no opportunistic cleanup
- measurements should be reproducible with checked-in commands
- keep the output concise and oriented toward roadmap execution

## Acceptance

- a baseline log exists with line-count and scan evidence
- `state_command.rs` and `effigy-release/src/lib.rs` status are explicitly
  recorded
- the next reduction card order is still justified by current evidence

## Completed

- Captured the baseline log at
  [`../../../logs/archive/2026-05/14-200500-g06-baseline-size-and-duplication.md`](../../../logs/archive/2026-05/14-200500-g06-baseline-size-and-duplication.md).
- Recorded Rust LOC at `233,544`.
- Recorded broader source/config surface LOC at `236,893`.
- Confirmed warning-level god files remain:
  `state_command.rs` and `effigy-release/src/lib.rs`.
- Confirmed duplicate-block baseline at `96` findings with `8` high.
- Kept the current execution order because the first two cards still target the
  largest ownership seams directly.

## Suggested Validation

```bash
effigy scan god-files --json
effigy scan duplicate-blocks --json
rg --files src crates tests skills | wc -l
find src crates tests skills -type f -name '*.rs' -print0 | xargs -0 wc -l | tail -n 1
```

## Next Task

Execute `802`.
