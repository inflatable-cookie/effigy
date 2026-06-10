# Manager Operation Report Proof

Date: 2026-05-05

## Summary

Completed card `404`.

## Outcome

Tightened manager operation-report tests so non-status lifecycle reports prove
backend id, interrupt policy name, repo root, action, state, and cleanup
failure payload together.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-container-manager operation_report -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next

Card `405` adds execution-surface plan parity proof.
