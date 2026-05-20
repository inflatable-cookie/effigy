# Residual Maintainability Lane Opened

Date: 2026-05-19  
Roadmap: [`g07.064`](../../roadmaps/g07/064-residual-maintainability-hardening-suite.md)  
Batch card: [`1014`](../../roadmaps/g07/batch-cards/1014-open-residual-maintainability-lane.md)  
Strict lane: [`095`](../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- opened the reopened `g07` residual-maintainability tranche
- locked the current residual baseline before any new cleanup work
- classified the live target buckets for `1015` through `1020`

## Baseline

- `effigy scan god-files --json`
  - `3` findings
  - `0 critical`
  - `0 high`
  - `3 warning`
  - files:
    - `crates/effigy-codegraph/src/language/manifest/semantic.rs`
    - `crates/effigy-codegraph/src/tests.rs`
    - `src/runner/script_command/mod.rs`
- `effigy scan duplicate-blocks --json`
  - `111` findings
  - `0 critical`
  - `7 high`
  - `104 warning`
- `effigy test --plan`
  - `cargo nextest run`

## Target Buckets

- `1015`
  - `crates/effigy-codegraph/src/language/manifest/semantic.rs`
- `1016`
  - `crates/effigy-codegraph/src/tests.rs`
- `1017`
  - `src/runner/script_command/mod.rs`
- `1018`
  - high help-topic duplicate clusters:
    - `bootstrap.rs` + `demo.rs`
    - `container.rs` + `release.rs`
- `1019`
  - high language-emitter duplicate clusters across:
    - `javascript.rs`
    - `php.rs`
    - `python.rs`
- `1020`
  - high runner-private fixture duplicate:
    - `src/runner/container_command/lifecycle.rs`
    - `src/runner/container_command/shell_prep.rs`

## Worktree Posture

- dirty worktree remains expected
- the active diff still includes the completed `g07.056` through `g07.063`
  implementation and closeout work plus the new reopened-lane planning files
- no unrelated changes were reverted

## Vision Target Delta

- primary vision tags touched: `MAINT`
- moved:
  - the reopened `g07` tranche now has a fixed residual baseline and explicit
    owner buckets
  - continuation is deterministic again through `095` and `1015`
- remains open:
  - `1015` through `1021`

## Next Task

Execute `1015`.
