# Patch Release Candidate Proof

Status: complete
Created: 2026-08-06
Roadmap: g08.026
Batch: patch-release-candidate-proof

## Summary

- Passed all six configured Effigy release gates without bypass.
- Confirmed release status selects patch `0.9.1`, tag `v0.9.1`, and reports
  ready to prepare.
- Re-ran focused release unit/CLI, full Clippy, and formatting checks.
- Proved Signal can replace its floor-script lock workaround with
  `sync-files = ["Cargo.lock"]` after `0.9.1` is installed.
- Closed the lane without release or downstream mutation.

## Signal Proof

A disposable local Signal clone at `0.1.1` was configured with Cargo lock sync
and its `cargo update --workspace` floor-script block removed. Current Effigy
planned `Cargo.lock` as a prepared `0.1.2` sync mutation. Applying the same
workspace-only update changed 28 workspace lock entries from `0.1.1` to
`0.1.2`, changed no third-party or non-version lines, and passed
`cargo metadata --locked`.

Because `Cargo.lock` is part of the prepared mutation/file set, normal execute
commit ownership also removes Signal's need for a separate pre-release
lockfile commit. The disposable clone was moved to Trash after proof; the real
Signal checkout remained clean and untouched.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `four consumer-discovered release defects plus a flaky
  self-gate and ambiguous source-drift recovery` -> current `repeatable gates,
  explicit source identity policy, and a ready 0.9.1 candidate`
- Remaining gap: explicit human authorization for release prepare/execute;
  downstream workaround removal after the released binary is installed

## Validation Performed

- `./target/debug/effigy release gates`
  - result: 6 of 6 passed; build, format, metadata, qa, smoke, test; 124002ms
- `cargo test -q -p effigy-release`
  - result: 16 passed
- `cargo test -q --test cli_output_tests release`
  - result: 65 passed
- `cargo clippy --all-targets -- -D warnings`
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `./target/debug/effigy release status`
  - result: patch `0.9.1`, tag `v0.9.1`, ready to prepare; gates not checked in
    the status invocation
- Signal disposable proof
  - result: 28 workspace versions moved, 0 foreign changes, locked metadata pass

## Boundaries

No release prepare, execute, tag, push, workflow edit, gate bypass, real Signal
mutation, or downstream workaround removal ran.

## Next Task

Request explicit human authorization before `effigy release prepare` or any
later release mutation.
