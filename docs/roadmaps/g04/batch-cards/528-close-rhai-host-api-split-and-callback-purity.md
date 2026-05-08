# 528 - Close Rhai Host API Split And Callback Purity

Lane: [`048-rhai-host-api-split-and-callback-purity-strict-lane.md`](../048-rhai-host-api-split-and-callback-purity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Review the Rhai host split, confirm callback purity acceptance, and close
`g04.006` or select the next focused cleanup.

## Scope

- run a final Rhai host file-size and module ownership audit
- confirm `exec::run` remains `TaskExecutionRequestBuilder` backed
- confirm Rhai container-sensitive callbacks no longer call
  `run_container_exec_capture*` directly
- update `g04.006` and strict-lane docs with closeout status if acceptance is
  met
- select `g04.007` as the next roadmap if `g04.006` closes

## Non-Goals

- no Rhai public API changes
- no broad runtime/container refactor
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.006` is either closed with validation evidence
or an explicit next Rhai cleanup card is selected.

## Closeout

`g04.006` acceptance is met:

- `host_api.rs` is a 118-line registry/runtime shell.
- no Rhai host module file is over 500 lines.
- `exec::run` remains backed by `TaskExecutionRequestBuilder`.
- Rhai container exec callbacks no longer call `run_container_exec_capture*`
  directly.

The next roadmap is `g04.007`, effective container policy decomposition.

## Validation

- `wc -l crates/effigy-rhai/src/host_api.rs crates/effigy-rhai/src/host_api/*.rs`
- `rg -n 'TaskExecutionRequestBuilder|run_container_exec_capture|Command::new\("(docker|colima|nerdctl)"' crates/effigy-rhai/src src/runner/script_command/mod.rs`
- `git diff --check`

## Next Task

Start card
[`529-scaffold-effective-container-policy-decomposition-lane.md`](./529-scaffold-effective-container-policy-decomposition-lane.md).
