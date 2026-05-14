# 2026-05-14 16:29:01 - Provider Source Materialization Convergence

Roadmap: [`g05.020`](../roadmaps/g05/020-reusable-core-hardening-suite.md)  
Batch card: [`743`](../roadmaps/g05/batch-cards/743-converge-provider-source-materialization.md)  
Strict lane: [`083`](../specs/083-reusable-core-hardening-strict-lane.md)

## What Changed

- added shared git source helper module at
  `crates/effigy-core/src/git_source.rs`
- moved canonical git cache identity, cache-segment sanitization, and sha256
  hashing there
- rewired bundle git source cache layout in
  `crates/effigy-manifest/src/bundles/source.rs`
- rewired deploy-provider git source cache layout in
  `src/runner/deploy_command/provider_package.rs`
- added runner integration proof that git-backed provider packages materialize
  under the shared cache identity rules

## Notes

- checkout behavior and provider error mapping stay local to deploy code
- OCI provider-package materialization remains explicitly unsupported in this
  slice
- duplicate-block scan dropped from 99 findings to 98 and no longer reports the
  prior manifest/provider git cache identity duplication

## Validation

- `cargo test -p effigy-core git_source`
- `cargo test -p effigy-manifest canonical_git_cache_identity_normalizes_common_remote_forms`
- `cargo test -p effigy-manifest git_bundle_source_materializes_into_shared_cache_root`
- `cargo test run_deploy_plan_materializes_git_provider_package_under_shared_cache_identity`
- `cargo test run_deploy_plan_json_reports_render_provider_preflight`
- `cargo fmt --all -- --check`
- `git diff --check`
- `effigy scan duplicate-blocks --json`
