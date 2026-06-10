# Bootstrap Target Repo Path Proof

Date: 2026-05-05

## Summary

Completed card `402`.

## Outcome

The full bootstrap CLI integration test now records task cwd for root setup,
child setup, and start execution. It proves all three run from cloned target
repos rather than the invocation cwd.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test --test bootstrap_cli_tests bootstrap_executes_root_child_and_start_via_binary -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test --test bootstrap_cli_tests -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`

## Next

Card `403` adds the inside-container re-entry context proof.
