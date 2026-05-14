# 745 - Finish State Domain Thin-Shell Follow-Through

Roadmap: [`../024-state-domain-extraction-follow-through.md`](../024-state-domain-extraction-follow-through.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Shrink `state_command.rs` and `effigy-state` by moving the next pure state
owner seams into dedicated state-domain modules.

## Scope

- classify the remaining large state surfaces by owner
- move pure report/path/history/planning behavior into `effigy-state`
- keep runner behavior at the CLI/render/side-effect edge

## Acceptance

- `state_command.rs` becomes meaningfully thinner
- `effigy-state` owns more durable state concepts in smaller modules
- state output compatibility remains intact

## Outcome

- split `crates/effigy-state/src/lib.rs` into focused domain modules for model,
  lineage, paths, history, apply, capture, validation, and tests
- moved state report-path helpers and pure context-file builders behind the
  `effigy-state` public API
- switched `src/runner/state_command.rs` to use the shared state-domain path
  and context builders
- reduced `crates/effigy-state/src/lib.rs` from a god-file-sized monolith to a
  small re-export surface
- reduced `src/runner/state_command.rs` from 2237 lines in the audit baseline
  to 2150 lines after this extraction batch

## Retained Large Surface

- `src/runner/state_command.rs` remains warning-sized because it still owns
  CLI dispatch, manifest loading, task execution, artifact staging, SQL import,
  hook execution, report writes, and text rendering
- that remaining surface is now mostly side-effect orchestration rather than
  shared pure state-domain logic

## Stop Conditions

- stop if a slice needs new state behavior rather than owner cleanup

## Validation

- `cargo test -p effigy-state`
- `cargo test state_command`
- `effigy scan god-files --json`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `746`.
