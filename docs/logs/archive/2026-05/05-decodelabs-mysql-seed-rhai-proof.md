# DecodeLabs Mysql Seed Rhai Proof

Date: 2026-05-05

## Summary

Completed card `400`.

## Outcome

Added a synthetic Rhai proof for the DecodeLabs mysql seed failure mode. The
test requires container-targeted `exec::run(...)` to preserve `stdin_file` and
route through the container callback.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai decodelabs_mysql_seed -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-rhai -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next

Card `401` adds the Underlay generated-compose path proof.
