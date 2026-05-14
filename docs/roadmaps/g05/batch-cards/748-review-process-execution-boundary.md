# 748 - Review Process Execution Boundary

Roadmap: [`../027-process-execution-boundary-review.md`](../027-process-execution-boundary-review.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14
Completed: 2026-05-14

## Purpose

Classify subprocess call sites and add only the minimum shared process helper
surface needed for predictable diagnostics and redaction.

## Scope

- inventory direct process creation call sites
- classify retained domain-owned calls versus shared utility candidates
- centralize only repeated error/result/redaction behavior where justified

## Acceptance

- direct subprocess call sites are classified
- any shared helper is minimal and justified
- retained direct calls are explicitly documented

## Stop Conditions

- stop if the lane starts drifting toward a mega process facade

## Result

- inventoried current direct subprocess ownership with `rg`
- added `effigy-core::git_exec` as the minimum shared helper for repeated git
  output/error projection
- switched manifest bundle git source materialization and deploy-provider git
  source materialization onto that helper
- documented why Rhai host execution, `effigy-process`, release git helpers,
  gateway/elevation paths, and distribution/runtime/container direct calls stay
  domain-owned for now

## Validation

- `cargo test -p effigy-core git_exec`
- `cargo test -p effigy-manifest git_bundle_source_materializes_into_shared_cache_root`
- `cargo test -p effigy-manifest canonical_git_cache_identity_normalizes_common_remote_forms`
- `cargo test run_deploy_plan_materializes_git_provider_package_under_shared_cache_identity`
- `cargo test run_deploy_plan_json_reports_render_provider_preflight`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `749`.
