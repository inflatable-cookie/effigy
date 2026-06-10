# Container Inspection Invocations Behind Manager

Date: 2026-05-05

## Summary

Completed card `394`.

## Outcome

`crates/effigy-containers/src/exec.rs` now uses
`ContainerManager::runtime_process_invocation(...)` for runtime `ps`,
`inspect`, and `stats` command shape.

The existing parsing and reporting code stayed in place.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `rg "resolve_compose_backend\(\)|ComposeBackend" crates/effigy-containers/src/exec.rs -n`

## Next

Card `395` decides the remaining backend-branching cleanup boundary.
