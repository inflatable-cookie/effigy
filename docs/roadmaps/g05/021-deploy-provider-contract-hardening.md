# g05.021 - Deploy Provider Contract Hardening

Status: Planned
Depends on: `g05.020`

## Goal

Make deploy-provider packages a stable plugin-like surface by tightening the
context and report contracts without moving provider-specific behavior back
into core Effigy.

External provider repos should be able to implement Render, Railway, and future
providers with full capability through Rhai and plain data contracts.

## Evidence

- provider report structs live in `src/runner/deploy_command/provider_package.rs`
  and currently validate schema, provider, and phase only
- provider report `status` and check statuses are plain strings
- provider context is assembled with `serde_json::json!` in
  `src/runner/deploy_command/provider_context.rs`
- provider phase execution writes context/report files and passes their paths
  through `EFFIGY_DEPLOY_PROVIDER_CONTEXT` and
  `EFFIGY_DEPLOY_PROVIDER_REPORT`
- `docs/contracts/025-deploy-provider-package-contract.md` already says core
  Effigy must expose enough Rhai API for provider packages to avoid shell glue

## Scope

- introduce typed serializable provider context structs for the current
  `effigy.deploy-provider.context.v1` wire shape
- introduce typed report/status/check validation for
  `effigy.deploy-provider.report.v1`
- add golden JSON tests for export, preflight, apply, and status context shapes
- add negative tests for invalid report schema, provider, phase, status, and
  check status values
- document the exact allowed report statuses and check statuses
- verify `export_path`, `plan`, `model`, deploy state, artifact policy, release
  policy, provider metadata, and provider project fields remain available to
  Rhai scripts
- keep Render/Railway behavior external; use local fixture providers only

## Out Of Scope

- no Render or Railway Rust code
- no provider-specific model derivation
- no provider provisioning behavior
- no secret syncing to provider accounts
- no migration of external provider repo code unless explicitly requested
- no breaking JSON field rename

## Guardrails For A Cheaper Model

- treat the JSON wire shape as public API
- prefer adding validation around existing shapes over renaming fields
- if a report field is currently accepted and harmless, keep it additive unless
  the contract says otherwise
- do not make provider packages link against Rust crates
- do not require external providers to call `effigy` recursively from Rhai
- if a provider script needs raw shell/env access, first check whether a typed
  Rhai helper already exists; if not, document the missing helper before adding
  more process glue

## Suggested Implementation Steps

1. Map the current provider context/report JSON emitted by tests.
2. Add Rust types that serialize to the same context shape.
3. Replace ad hoc context construction with the typed serializer.
4. Add report status and check status enums or validators.
5. Add golden tests for context JSON and invalid report rejection.
6. Update the provider package contract doc with allowed values and examples.
7. Run focused deploy/provider and Rhai tests, then run a wider test pass.

## Acceptance Criteria

- provider context JSON remains backward compatible for existing external
  provider scripts
- invalid report statuses fail with clear diagnostics
- valid fixture provider reports pass for export, preflight, apply, and status
- the provider package contract doc matches implementation
- no product/provider-specific code appears in core Rust

## Validation

Minimum focused validation:

```bash
cargo test deploy_provider
cargo test -p effigy-rhai provider
effigy deploy export render --plan --json
```

Use fixture providers for command tests. Only run real external provider repos
if the user asks.

## Next Task

After this lands, move to `g05.022` so provider package delivery uses the same
source-materialization discipline as bundles.
