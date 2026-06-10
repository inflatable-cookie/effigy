# Rhai Secrets And Process Support Split

Date: 2026-05-14

## Summary

Completed card `730`, the first Rhai internal boundary follow-through slice.

## Changes

- added `crates/effigy-rhai/src/rhai_secrets.rs`
- added `crates/effigy-rhai/src/process_support.rs`
- moved Rhai secret-store and vault mutation logic into the new secrets owner
- moved process execution and streaming helpers into the new process-support
  owner
- reduced `effigy-rhai/src/lib.rs` to thin delegating wrappers for those seams
- advanced current ready work to card `731`

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`, `CONTRACT`
- Baseline: `effigy-rhai/src/lib.rs` still mixed secret-store ownership and
  process execution support into one large crate facade.
- Current state: those two concerns now live behind dedicated internal modules
  while the public Rhai host surface stays stable.
- Remaining open: Rhai streaming/search/http owner follow-through, CLI help
  convergence, fixture dedup, docs reference refresh, and final closeout.

## Validation

- `cargo test -p effigy-rhai execute_rhai_script_exposes_declared_rhai_secrets`
- `cargo test -p effigy-rhai execute_rhai_script_process_helpers_accept_cwd_and_env_options`
- `cargo fmt --all -- --check`
- `git diff --check`

## Validation Blockers

- `cargo test -p effigy-rhai` still fails on two pre-existing script policy
  checks:
  - `first_party_rhai_process_calls_are_allowlisted`
  - `first_party_rhai_scripts_do_not_use_legacy_module_dot_calls`

These failures come from existing first-party `.rhai` scripts outside the `730`
implementation seam.

## Next Task

Execute `731` to extract Rhai streaming, search, and HTTP support modules.
