# Container Lifecycle Manager Reports

Date: 2026-05-05

## Change

Completed card `384`.

Lifecycle paths now create internal manager operation reports through
`effigy-runtime::container_manager` for up, down, status, stats, and logs.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-containers-target cargo test -p effigy-containers compose -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_command -- --nocapture`

## Next Task

Implement card `385`: migrate exec, copy, and data operations through
`ContainerManager`.
