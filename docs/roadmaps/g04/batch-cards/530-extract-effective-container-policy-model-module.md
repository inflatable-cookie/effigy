# 530 - Extract Effective Container Policy Model Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move effective container policy model types out of `crates/effigy-containers/src/lib.rs`
into a focused policy model module without changing public exports.

## Scope

- create `crates/effigy-containers/src/policy/mod.rs`
- create `crates/effigy-containers/src/policy/model.rs`
- move model-only types from `lib.rs`:
  - `EffectiveAttachMode`
  - `EffectiveComposeSource`
  - `EffectiveContainerPolicy`
  - `EffectiveHostProcess`
  - `HostProcessRestart`
  - `HostProcessSignal`
  - `EffectiveDnsRoute`
  - `EffectiveServiceAlias`
  - `SharedServiceBinding`
  - `ContainerPolicyError`
  - `ContainerEjectResult`
- keep existing public exports stable through `lib.rs`
- avoid behavior changes

## Non-Goals

- no policy loading split
- no workspace split
- no validation split
- no public API break
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when model types live under `policy/model.rs`, existing
imports still compile, and focused container tests pass.

## Closeout

Effective container policy model types now live under
`crates/effigy-containers/src/policy/model.rs` and remain publicly re-exported
from `crates/effigy-containers/src/lib.rs`. `lib.rs` dropped from 1563 to 1379
lines.

The full parallel `effigy-containers` crate test hit an existing global `PATH`
mutation race in two exec timeout tests. The same crate suite passed serially.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-model-check cargo check -p effigy-containers`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-model-libcheck cargo check -p effigy --lib`
- `CARGO_TARGET_DIR=/tmp/effigy-g04-policy-model-test-serial cargo test -p effigy-containers -- --test-threads=1`
- `git diff --check`

## Next Task

Start card
[`531-extract-effective-container-policy-project-module.md`](./531-extract-effective-container-policy-project-module.md).
