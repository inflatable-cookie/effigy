# 742 - Harden Deploy Provider Context And Report Contracts

Roadmap: [`../021-deploy-provider-contract-hardening.md`](../021-deploy-provider-contract-hardening.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Make deploy-provider packages a stable plugin-like surface by replacing ad hoc
provider context/report handling with stricter typed ownership and proof.

## Scope

- add typed provider context structs for the current
  `effigy.deploy-provider.context.v1` wire shape
- add stricter provider report validation for schema, provider, phase, status,
  and check status values
- add golden tests for export, preflight, apply, and status context/report
  shapes
- keep provider-specific behavior outside core

## Acceptance

- provider context JSON remains backward compatible
- invalid provider reports fail with clear diagnostics
- fixture provider reports pass for export, preflight, apply, and status
- docs/contracts match the validated wire shape

## Completed

- replaced ad hoc provider context JSON assembly with typed serializable context
  owners while preserving the current wire shape
- added enum-backed provider report and check statuses so unknown values fail
  during provider report parsing
- added phase-bounded report status validation for `preflight`, `apply`,
  `status`, and `export`
- added deploy tests proving `export_path` and `plan` are present in export
  provider context
- added negative deploy tests proving invalid provider report and check statuses
  are rejected
- updated the deploy-provider package contract with explicit allowed status
  values and the missing `export_path` and `plan` context fields

## Validation

- `cargo test run_deploy_plan_rejects_invalid_provider_report_status`
- `cargo test run_deploy_plan_rejects_invalid_provider_check_status`
- `cargo test run_deploy_export_context_includes_export_path_and_plan`
- `cargo test run_deploy_plan_json_reports_render_provider_preflight`
- `cargo test deploy_provider`
- `cargo test -p effigy-rhai execute_rhai_script_exposes_deploy_provider_context_and_report_helpers`
- `cargo fmt --all -- --check`
- `effigy docs check paths docs/contracts/025-deploy-provider-package-contract.md docs/roadmaps docs/specs`
- `git diff --check`

## Stop Conditions

- stop if a required provider report field is not actually stable enough for a
  closed contract
- stop if external provider repos need a wire-shape break instead of additive
  tightening

## Next Task

Execute `743`.
