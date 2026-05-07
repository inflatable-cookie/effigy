# 259 Implement Runner Test Prelude Split

Status: landed
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Shrink the runner test prelude surface so `src/tests/runner_tests/prelude.rs`
and `prelude/managed.rs` stop acting like god-modules for fixtures,
assertions, case tables, and runtime glue.

## Context

The earlier prelude flatten helped, but the result is still one large import
hub plus one large managed helper file. That keeps test imports simple while
hiding ownership and making later cleanup harder.

The target is still a simple top-level test surface, but backed by smaller
owned helper modules.

## In Scope

- Split `src/tests/runner_tests/prelude/managed.rs` into smaller local
  modules such as `managed/{cases,fixtures,assertions,helpers}`.
- Reduce the facade and re-export wall inside
  `src/tests/runner_tests/prelude.rs`.
- Keep test call sites readable; do not reintroduce a deep nested-prelude
  chain.
- Keep behavior and assertions unchanged.

## Out Of Scope

- Product code changes.
- Broad test renaming churn with no ownership benefit.
- Reopening crate-boundary decisions for non-test code.

## Acceptance Criteria

- `prelude/managed.rs` is replaced by smaller owned modules.
- `prelude.rs` is slimmer and less facade-heavy than the current form.
- Test behavior stays unchanged.
- Standard validation round passes for the batch.

## Next Task

Reassess the lane after the four-card `/src` cleanup chain lands, then decide
whether `g02.010` should pause, reopen built-in/routing-core extraction
planning, or hand back to release execution.
