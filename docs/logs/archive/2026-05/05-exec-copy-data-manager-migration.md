# Exec Copy Data Manager Migration

Date: 2026-05-05

## Change

Completed card `385`.

Runner exec, copy, data, shared compose, runtime volume, and generated image
removal paths now use manager-owned backend selection and Docker/Colima process
wrapping.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_command -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy exec_command -- --nocapture`
- `rg "resolve_compose_backend|ComposeBackend" src/runner/exec_command src/runner/container_command crates/effigy-runtime/src/write.rs -n`

## Next Task

Implement card `386`: close `g03.031` with drift guards and contract/readme
alignment.
