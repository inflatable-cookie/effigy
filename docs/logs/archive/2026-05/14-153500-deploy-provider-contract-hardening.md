# Deploy Provider Contract Hardening

Date: 2026-05-14

## Summary

Completed card `742`.

## Changes

- replaced deploy-provider context JSON assembly with typed serializable
  context owners
- added enum-backed provider report and check statuses
- added phase-bounded provider report status validation
- added deploy tests proving export context includes `export_path` and `plan`
- added negative deploy tests for invalid provider report and check statuses
- updated the deploy-provider package contract with explicit allowed statuses
  and the full context shape used by export
- advanced lane `083` so `743` is now the ready slice

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Baseline: provider package report parsing only validated schema, provider,
  and phase, and provider context was assembled through ad hoc JSON literals.
- Current state: provider contract parsing rejects unknown status values, export
  context shape is typed and proved, and the active contract now names the
  allowed report and check status sets explicitly.

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

## Next Task

Execute `743`.
