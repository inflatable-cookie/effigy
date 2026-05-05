# Execution Surface Plan Parity Proof

Date: 2026-05-05

## Summary

Completed card `405`.

## Outcome

Added a focused `effigy-execution` proof that direct CLI, bootstrap, and Rhai
surfaces produce equivalent resolved plans for equivalent selector, context,
runtime policy, env, cwd, and `stdin_file` inputs.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution direct_bootstrap_and_rhai -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next

Card `406` decides whether `g03.034` can close.
