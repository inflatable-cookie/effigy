# 535 - Extract Generated Compose Eject Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move generated compose eject ownership out of
`crates/effigy-containers/src/lib.rs` into a focused runtime module without
changing the public eject helper.

## Scope

- create `crates/effigy-containers/src/runtime/eject.rs`
- move `eject_generated_compose`
- keep `eject_generated_compose` re-exported from `lib.rs`
- keep generated compose output directories stable
- preserve error text

## Non-Goals

- no policy loading split
- no workspace module split
- no runtime DNS changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when generated compose eject logic lives outside
`lib.rs`, eject tests pass, and public callers still compile.

## Closeout

Generated compose eject logic now lives under
`crates/effigy-containers/src/runtime/eject.rs` and remains re-exported from
`lib.rs`. `lib.rs` dropped from 641 to 621 lines.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-eject-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-eject-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-eject-test cargo test -p effigy-containers eject_generated_compose -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`536-extract-container-policy-load-module.md`](./536-extract-container-policy-load-module.md).
