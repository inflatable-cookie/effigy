# 052 - Drift Guards And Architecture Proof Matrix Strict Lane

Roadmap: [`g04.010`](../roadmaps/g04/010-drift-guards-and-architecture-proof-matrix.md)

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

Prevent runtime/container logic soup from returning by adding lightweight,
explainable drift guards and a focused proof matrix for critical paths.

## Hard Boundaries

- preserve public CLI behavior
- keep guards explainable and easy to suppress deliberately
- prefer focused `rg` or docs-check guards before heavier framework work
- no release work
- no `.github/workflows/` edits

## Current Ready Card

[`572-close-drift-guards-and-handoff-contract-promotion.md`](./batch-cards/572-close-drift-guards-and-handoff-contract-promotion.md)

## Execution Chain

- `567` complete: close CLI parser modularisation and hand off to drift guards
- `568` complete: scaffold drift guards and proof matrix lane
- `569` complete: add runtime/container drift guard task
- `570` complete: add runtime/container proof matrix inventory
- `571` complete: add exec/workspace/managed proof coverage
- `572` ready: close drift guards and hand off to contract promotion

## Existing Guard Surfaces

- `config/tasks.toml` owns local QA aggregators:
  - `qa:docs`
  - `qa:json`
  - `qa:ci`
  - `qa`
- `qa:docs:agent-defaults` already uses `docs check-forbidden` for simple
  text drift.
- `effigy docs check-*` covers docs links, JSON examples, headings, paths,
  contains, forbidden text, indexes, next-action policy, workflow paths, and
  log index updates.
- `effigy contracts check-json` and `contracts validate-selection` cover JSON
  contract drift.
- `effigy scan` covers god files, duplicate blocks, comment ratio, generated
  assets, generated-in-src, attention markers, and stale suppressions.
- `config/scan.toml` configures repo scan thresholds.
- `scripts/rhai/write-json-contract-artifacts.rhai` owns repo-local JSON
  contract artifact capture for CI validation tasks.
- `scripts/check-linux-glibc-floor.sh` remains a release/distribution-specific
  shell guard.

## Initial Guard Targets

- no direct `std::env::current_dir()` in `src/runner/**`
  - current allowance: `src/runner/exec_command/tests.rs`
- no runner-local `Command::new("docker"|"colima"|"nerdctl")`
  - current allowances:
    `src/runner/doctor_ports.rs`,
    `src/runner/bootstrap_command/mod.rs`
- no runner calls to `resolve_compose_backend` or `ComposeBackend`
  - current allowances:
    `src/runner/doctor_ports.rs`,
    `src/runner/exec_command/transport.rs`,
    `src/runner/container_runtime_prep/prep.rs`,
    `src/runner/container_command/lifecycle.rs`
- no runner calls to `compose_args` outside allowed adapter modules
  - current allowances:
    `crates/effigy-runtime/src/container_manager.rs`,
    `src/runner/managed_shell.rs`,
    `src/runner/demo_command/execute/task/selection.rs`,
    `src/runner/exec_command/transport.rs`,
    `src/runner/exec_command/transport/colima.rs`,
    `src/runner/container_runtime_prep/prep.rs`,
    `src/runner/container_runtime_prep/mod.rs`,
    `src/runner/container_command/support.rs`,
    `src/runner/deferral/run.rs`,
    `src/runner/execute/pipeline/standard.rs`,
    `src/runner/execute/pipeline/managed.rs`,
    `src/runner/system_command/workspace_provisioning.rs`
- no runner calls to `run_docker_capture` outside allowed adapter modules
  - current allowance: none
- no legacy container exec capture callers outside known migration debt
  - current allowances:
    `src/runner/db_seed.rs`,
    `src/runner/container_command/data.rs`,
    `src/runner/container_command/lifecycle.rs`,
    `src/runner/container_command/mod.rs`
- no Rhai container-sensitive helpers bypassing execution/container operation
  requests
  - current allowance: none
- no new god file over threshold without explicit suppression

## Suppression Policy

- Prefer removing the drift over adding an allowance.
- Temporary allowances live in `scripts/check-runtime-container-drift.sh` and
  must also be named in this lane.
- Allowances should be path-scoped, not broad pattern suppressions.
- Each allowance should map to a future migration card or remain clearly marked
  as an adapter boundary.

## Initial Proof Areas

- direct CLI task routes
- bootstrap task dispatch
- Rhai task and container-sensitive dispatch
- run-array dispatch
- demo task re-entry
- container data seed/dump
- `effigy exec`
- workspace sessions
- managed dev activation

## Proof Matrix Inventory

| Proof area | Existing coverage | Gap | First next action |
| --- | --- | --- | --- |
| direct CLI host task route | `crates/effigy-execution` request/dispatch plan tests and runner execute tests cover host route selection and request construction. | Keep as regression surface; no immediate gap from this lane. | None. |
| direct CLI container task route | Standard/managed pipeline unit tests cover container binding, activation-plan construction, and stay-in-shell decisions. | Coverage is split across runner files and still leans on old activation glue. | Add focused proof once the remaining activation debt is migrated. |
| inside-container handoff route | `crates/effigy-context` captures container handoff, and standard/managed pipeline tests cover handoff-specific route/rendering decisions. | No single proof ties handoff context through `effigy exec`, workspace, and managed surfaces. | Include in card `571`. |
| bootstrap task dispatch | Bootstrap parser and seed/dump option tests cover bootstrap surfaces; runtime-context contract covers cloned target repo behavior. | Need a focused proof that bootstrap task dispatch consumes the same task request/plan shape as direct CLI. | Defer to an execution parity card after `571`. |
| Rhai `exec::run(... run_in=container ...)` | `crates/effigy-rhai/src/host_api/exec.rs` builds through `TaskExecutionRequestBuilder`; tests cover host/container route JSON, `stdin_file`, and a DecodeLabs mysql seed-style container exec proof. | `container::*` callbacks still route through script-command callback glue and legacy container helpers. | Defer to Rhai callback migration debt card. |
| first-party Rhai scripts | `crates/effigy-rhai` tests forbid first-party scripts from using `container::exec(` for container commands. | Keep guard current as new scripts are added. | None. |
| run-array embedded dispatch | Run-array Rhai tests cover script task execution and container helper behavior. | Need parity proof that embedded dispatch cannot bypass `TaskExecutionRequestBuilder`. | Defer to execution parity card. |
| demo task re-entry | Demo execution path has selection/runtime route coverage through runner/demo code and parser/help tests. | Need proof tied to execution request parity rather than demo-local route assertions. | Defer to execution parity card. |
| container data seed local SQL | `effigy-data` tests cover local seed source normalization and handoff planning; parser/help tests cover CLI syntax. | Runner integration remains mixed with prompt/render glue. | Defer to data pipeline cleanup card. |
| container data seed OCI artifact | `effigy-data`, parser, bootstrap option, and DB seed tests cover `oci://` classification, staging handoff, and preservation. | Need end-to-end focused runner proof once artifact side effects are easier to fake. | Defer to data proof card. |
| container data dump local SQL | `effigy-data` tests cover local dump destination and DB command planning; parser/help tests cover CLI syntax. | Runner data command still owns too much dump orchestration. | Defer to data pipeline cleanup card. |
| container data dump OCI artifact | `effigy-data` and container data tests cover `oci://` dump destination classification, capture handoff, and `--push` rejection without OCI destination. | Need a proof that planned OCI capture remains explicit and manager-backed. | Defer to data proof card. |
| `effigy exec` container path | `src/runner/exec_command/tests.rs` covers exec strategy rendering, handoff args, workspace install strategy, activation-plan construction, and skip-refresh lease identity through `activate_exec_surface_preserves_skip_lease_policy_for_handoff_sessions`. | Manager-backed transport still has migration debt tracked by drift allowances. | Defer manager transport cleanup to a later migration card. |
| workspace sessions | `src/runner/system_command/workspace/tests.rs` covers cleanup ownership, seeded/bootstrap handoff cases, gateway preparation order, inline workspace rejection, and explicit repo override propagation through `workspace_handoff_preparation_preserves_explicit_repo_override`. | Workspace session activation still has older runner glue, but proof covers the brittle repo-target handoff edge. | Defer activation glue cleanup to a later migration card. |
| managed dev activation | `src/runner/execute/pipeline/managed.rs` tests cover activation-plan identity/lease policy, handoff rendering, lifecycle cleanup, DNS routes, shell guards, and unnamed-container policy identity through `managed_activation_plan_preserves_policy_identity_without_container_name`. | Remaining gap is mostly consolidation, not missing behavioral proof. | None for this lane. |
| Ctrl+C / attached closeout | Container manager/runtime session tests cover pieces of attached cleanup and lifecycle reporting. | Coverage is not visible from this matrix yet. | Inventory during closeout after card `571`. |

## Proof Slice Result

Card `571` added focused proof coverage around the three surfaces where the
existing tests were useful but scattered:

- `effigy exec` preserves repo override, container identity, and skip-refresh
  lease policy through activation planning
- workspace handoff preserves explicit repo override into provisioning
- managed activation preserves policy identity without relying on a named
  container

The remaining gaps are migration debt already visible in the drift guard
allowances, not missing proof rows for this lane.

## Exit Condition

This lane closes when focused guard commands exist, suppression policy is
documented, and the proof matrix covers the critical runtime/container paths.

## Next Task

Card
[`572-close-drift-guards-and-handoff-contract-promotion.md`](./batch-cards/572-close-drift-guards-and-handoff-contract-promotion.md).
