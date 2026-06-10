# Compose Backend Manager Migration

Date: 2026-05-05

## Change

Completed card `383`.

Compose backend detection and Docker-vs-Colima compose process wrapping now
route through `effigy-container-manager`. `effigy-containers::compose` remains
as a compatibility wrapper for existing callers.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-containers-target cargo test -p effigy-containers compose -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next Task

Implement card `384`: migrate container lifecycle commands through
`ContainerManager`.
