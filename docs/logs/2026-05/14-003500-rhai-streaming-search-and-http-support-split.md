# Rhai Streaming Search And HTTP Support Split

Date: 2026-05-14

## Summary

Completed card `731`, the second Rhai internal boundary follow-through slice.

## Changes

- added `crates/effigy-rhai/src/network_support.rs`
- moved Rhai search and HTTP helper implementations into the new support owner
- rewired host API modules to use the support owner directly
- removed the moved search and HTTP implementation bodies from
  `crates/effigy-rhai/src/lib.rs`
- advanced current ready work to card `732`

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`
- Baseline: `effigy-rhai/src/lib.rs` still carried search and HTTP helper logic
  after the earlier secrets and process split.
- Current state: Rhai search and HTTP support now live behind a dedicated
  internal support module, leaving the crate facade slimmer again.
- Remaining open: CLI help convergence, fixture dedup, docs reference refresh,
  and final closeout.

## Validation

- `cargo test -p effigy-rhai execute_rhai_script_can_search_files_without_rg`
- `cargo test -p effigy-rhai execute_rhai_script_can_make_http_requests`
- `cargo test -p effigy-rhai execute_rhai_script_can_stream_process_output`
- `cargo fmt --all -- --check`
- `git diff --check`

## Validation Blockers

- `cargo test -p effigy-rhai` still fails on the same pre-existing first-party
  script policy checks recorded after `730`.

## Next Task

Execute `732` to converge CLI help topic descriptors and registration.
