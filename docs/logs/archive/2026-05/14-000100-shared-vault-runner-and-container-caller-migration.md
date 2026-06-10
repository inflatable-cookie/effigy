# Shared Vault Runner And Container Caller Migration

Date: 2026-05-14

## Summary

Completed cards `725`, `726`, and `727` for the shared vault-access follow-through.

## Changes

- confirmed the shared vault-access lane and split the immediate work into
  runner-owned and later Rhai-owned slices
- added `src/runner/secret_vault.rs` for shared vault path and payload loading
- switched local secrets commands and task secret injection to the shared vault
  support
- switched container secret callers to the same shared vault support
- advanced current ready work to card `728`

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `MAINT`
- Baseline: local vault commands, task secret injection, and container secret
  injection each reimplemented the same vault path and payload-loading rules.
- Current state: those runner-owned callers now share one vault path and
  payload-loading support boundary, while Rhai adoption remains queued in the
  later crate-local boundary card.
- Remaining open: container lifecycle owner split, Rhai internal boundary work,
  CLI help convergence, fixture dedup, docs reference refresh, and final closeout.

## Validation

- `cargo test -p effigy secrets`
- `cargo test -p effigy task_env`
- `cargo test -p effigy container_secret_env`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `728` to split lifecycle-owned container secrets and shell prep out of
`lifecycle.rs`.
