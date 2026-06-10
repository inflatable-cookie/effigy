# Process Execution Boundary Review

Date: 2026-05-14
Roadmap: `g05.020`
Batch card: `748`

## What changed

- added `effigy-core::git_exec` for shared git subprocess output/error
  projection
- switched:
  - `crates/effigy-manifest/src/bundles/source.rs`
  - `src/runner/deploy_command/provider_package.rs`
  onto that helper
- reviewed remaining direct subprocess owners and recorded why they stay local

## Classification

### Shared helper adopted

- bundle git source materialization
- deploy-provider git source materialization

These two callers shared the same low-level subprocess behavior and only needed
command execution plus stderr-based failure rendering.

### Retained direct-call owners

- Rhai host process helpers:
  need redaction, PTY fallback, cwd/env/stdin option mapping, and live IO
- `effigy-process`:
  owns supervisor lifecycle and signal semantics
- release git helpers:
  keep release-specific diagnostics and safety language local
- gateway, distribution, doctor, runtime signal, and container execution paths:
  bind to platform-specific tools, privilege elevation, or transport-specific IO

## Outcome

The review did not justify a broad subprocess facade. The minimum useful shared
boundary was a git-command helper for duplicated output/error projection. The
rest of the call sites remain intentionally domain-owned.

## Validation

- `cargo test -p effigy-core git_exec`
- `cargo test -p effigy-manifest git_bundle_source_materializes_into_shared_cache_root`
- `cargo test -p effigy-manifest canonical_git_cache_identity_normalizes_common_remote_forms`
- `cargo test run_deploy_plan_materializes_git_provider_package_under_shared_cache_identity`
- `cargo test run_deploy_plan_json_reports_render_provider_preflight`
- `cargo fmt --all -- --check`
- `git diff --check`
