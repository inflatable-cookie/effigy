# Inside Container Reentry Context Proof

Date: 2026-05-05

## Summary

Completed card `403`.

## Outcome

Strengthened the `effigy-execution` handoff proof so a container-targeted
request inside an Effigy container handoff resolves locally while preserving
captured cwd and repo target authority.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution handoff_routes_locally -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next

Card `404` adds the manager operation-report proof.
