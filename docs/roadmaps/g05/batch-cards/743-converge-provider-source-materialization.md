# 743 - Converge Provider Source Materialization

Roadmap: [`../022-provider-source-materialization-convergence.md`](../022-provider-source-materialization-convergence.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Reduce duplicated path/git source materialization logic between bundle sources
and deploy-provider packages without coupling provider behavior to bundle code.

## Scope

- extract only shared cache identity, URL normalization, ref sanitization, and
  checkout primitives where the behavior is intentionally the same
- keep descriptor parsing and provider policy inside deploy code
- decide whether OCI stays explicitly unsupported here or moves forward with
  bounded proof

## Acceptance

- duplicated source-materialization logic is reduced materially
- provider package resolution stays provider-neutral
- OCI behavior is either explicitly unsupported with tests or implemented with
  tests

## Outcome

- moved shared git cache identity, ref sanitization, and hashing helpers into
  `crates/effigy-core/src/git_source.rs`
- switched bundle git source materialization and deploy-provider git source
  materialization to the shared helper without moving checkout or
  provider-specific behavior into shared code
- kept provider OCI behavior explicitly unsupported; this card did not widen
  source-materialization scope beyond the duplicated git path
- added targeted tests proving shared helper normalization and provider git
  cache materialization behavior

## Stop Conditions

- stop if the shared helper starts absorbing provider-specific or
  bundle-specific behavior

## Validation

- `cargo test -p effigy-core git_source`
- `cargo test -p effigy-manifest canonical_git_cache_identity_normalizes_common_remote_forms`
- `cargo test -p effigy-manifest git_bundle_source_materializes_into_shared_cache_root`
- `cargo test run_deploy_plan_materializes_git_provider_package_under_shared_cache_identity`
- `cargo test run_deploy_plan_json_reports_render_provider_preflight`
- `cargo fmt --all -- --check`
- `git diff --check`
- `effigy scan duplicate-blocks --json`

## Next Task

Execute `744`.
