# 2026-04-14 22:15:00 Effigy Rhai Dogfooding Cluster Implementation

## Summary

Implemented the first substantial Effigy-only Rhai dogfooding cluster after the
script-step foundation. This batch moved one operator task and one shipped demo
runner onto file-backed Rhai scripts, extended the host API only where the
first-party migration demanded it, and recorded the remaining lifecycle/signal
gap instead of hiding it behind a weaker rewrite.

## Shipped

- Migrated `smoke:release` onto `scripts/rhai/check-release-smoke.rhai`
- Migrated the `browser-proof-report` demo onto
  `scripts/rhai/write-browser-proof-report.rhai`
- Added Rhai host helpers:
  - `now_utc()`
  - `make_temp_dir(prefix)`
  - `write_lines(path, lines_array)`
- Removed the now-unused `scripts/demo/write-browser-proof-report.sh` shell
  entrypoint
- Extended Rhai tests to cover the new runtime helpers

## Gaps Exposed

- `lifecycle-window` remains shell-backed for now
- the blocking gap is signal-aware long-running script lifecycle behavior,
  especially cleanup/status writes on termination
- that gap is a better next slice than forcing another weak shell-to-Rhai port

## Validation

- `cargo test` passed
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity` passed
- `cargo run --bin effigy -- demo run browser-proof-report` passed
- `cargo run --bin effigy -- smoke:release target/debug/effigy` passed
- `cargo run --bin effigy -- qa:docs` passed
- `git diff --check` passed

## Next Task

Decide whether the next Rhai slice should be:

- another Effigy dogfooding batch
- a bounded host-API expansion for signal-aware long-running scripts
- or the first Keepsake pilot
