# 006 - Rhai Host API Split And Callback Purity

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`005-data-seed-dump-pipeline.md`](./005-data-seed-dump-pipeline.md)

## Goal

Make Rhai host APIs modular and route runtime-sensitive work through typed
pipeline requests.

## Scope

- split `host_api.rs` by module
- separate pure Rhai conversion helpers from side-effect callbacks
- route `exec::run(...)` through `TaskExecutionRequestBuilder`
- route `container::*` helpers through `ContainerOperationRequest`
- add module-level tests
- preserve current Rhai public API unless a card explicitly documents a break

## Migration Targets

- `crates/effigy-rhai/src/host_api.rs`
- `crates/effigy-rhai/src/lib.rs`
- `src/runner/script_command/mod.rs`
- `docs/guides/061-rhai-script-steps-guide.md`
- `docs/guides/068-rhai-host-surface-audit.md`

## Acceptance Criteria

- no Rhai host module file over 500 lines
- callback surface is typed by domain
- container-sensitive Rhai helpers do not call runner container helpers
  directly
- DecodeLabs mysql seed proof remains passing

## Validation

- `cargo test -p effigy-rhai`
- `cargo test -p effigy --lib script_command`
- DecodeLabs mysql seed proof test

## Next Task

Start roadmap
[`g04.007`](./007-effective-container-policy-decomposition.md).
