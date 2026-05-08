# 2026-05-05 - Runtime Context Lane Opened

## Summary

Opened the runtime/container/execution modularisation runway and landed the
first `g03.030` runtime-context slice.

## Changed

- added `crates/effigy-context`
- added `docs/contracts/011-runtime-context-contract.md`
- opened roadmaps `g03.030` through `g03.035`
- opened strict lane `036`
- completed card `374`
- wired CLI dispatch through `EffigyRuntimeContext`
- added lossy context capture for non-repo CLI fallback paths

## Boundary

The existing dirty DecodeLabs files were left untouched.

Additional unrelated dirty files appeared during validation:

- `crates/effigy-rhai/src/lib.rs`
- `crates/effigy-rhai/src/tests.rs`

Those were not part of this batch.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-context-target cargo test -p effigy-context -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `./target/debug/effigy docs check-links docs/contracts/README.md docs/contracts/011-runtime-context-contract.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/030-universal-runtime-context-and-path-authority.md docs/specs/036-universal-runtime-context-and-path-authority-strict-lane.md docs/roadmaps/g03/batch-cards/374-plan-runtime-context-contract-and-crate-boundary.md`
- `git diff --check`

## Next

Create the next `g03.030` migration card for command-local cwd/root callers.
