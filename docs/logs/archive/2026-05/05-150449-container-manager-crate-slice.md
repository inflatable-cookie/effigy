# Container Manager Crate Slice

Date: 2026-05-05

## Change

Completed card `382`.

Added `crates/effigy-container-manager` with the first manager facade,
registry, backend trait, Docker Compose backend stub, Colima/nerdctl backend
stub, typed operation reports, and interrupt policy.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Implement card `383`: move compose backend detection behind
`ContainerManager`.
