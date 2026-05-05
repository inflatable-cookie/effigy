# Colima Start Runtime Selection Behind Manager

Date: 2026-05-05

## Summary

Completed card `396`.

## Outcome

Colima start command assembly now uses `ContainerManager::colima_start_runtime`
for `--runtime` selection instead of reaching through the legacy compose
backend enum.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers colima -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `rg "resolve_compose_backend\(\)|ComposeBackend" crates/effigy-containers/src/colima.rs -n`

## Note

The Colima env override tests now serialize env mutation to remove a parallel
test race exposed by this validation slice.

## Next

Card `397` decides the `g03.033` closeout boundary.
