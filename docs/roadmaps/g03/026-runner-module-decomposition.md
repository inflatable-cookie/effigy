# 026 - Runner Module Decomposition

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-03
Depends on: —

## Progress

- `deploy_command.rs` (1,340 lines) → split into 5 modules:
  - `mod.rs` (154 lines) — entry point and export dispatch
  - `derive.rs` (389 lines) — model derivation from bundle config
  - `model.rs` (329 lines) — all struct/enum definitions
  - `render.rs` (223 lines) — Render export logic
  - `railway.rs` (254 lines) — Railway export logic

- `exec_command.rs` (975 lines) → split into 4 modules:
  - `mod.rs` (435 lines) — entry point and public API
  - `surface.rs` (283 lines) — exec surface selection and resolution
  - `transport.rs` (305 lines) — transport dispatch and compose exec
  - `tests.rs` (545 lines) — all tests

- `bootstrap_command.rs` (869 lines) → split into 3 modules:
  - `mod.rs` (~621 lines) — main bootstrap flow
  - `deps.rs` (~258 lines) — deps sync subsystem
  - `tests.rs` (655 lines) — all tests

- `container_runtime_prep.rs` (929 lines) → split into 2 modules:
  - `mod.rs` (~334 lines) — runtime prep logic
  - `tests.rs` (~596 lines) — all tests

- `gateway_command.rs` (850 lines) → split into 2 modules:
  - `mod.rs` (~671 lines) — gateway command logic
  - `tests.rs` (~179 lines) — all tests

- `script_command.rs` (833 lines) → split into 2 modules:
  - `mod.rs` (~784 lines) — script command logic
  - `tests.rs` (~48 lines) — all tests

## Remaining Work

All identified oversized modules have been decomposed. No runner module exceeds ~800 lines.

## Problem

Several files in `src/runner/` have grown large enough to hinder navigation and
review:

- `src/runner/deploy_command.rs` — 1,340 lines
- `src/runner/container_command/gateway_registration.rs` — 1,177 lines
- `src/runner/exec_command.rs` — 975 lines
- `src/runner/container_runtime_prep.rs` — 929 lines
- `src/runner/gateway_command.rs` — 850 lines
- `src/runner/script_command.rs` — 833 lines
- `src/runner/bootstrap_command.rs` — 826 lines

These modules mix multiple responsibilities (model, export, provider logic,
runtime prep, etc.) in single files.

## Goal

Decompose the largest runner modules into smaller, focused submodules without
changing behavior.

## Scope

- split `deploy_command.rs` into model, export, and provider-boundary submodules
- split `gateway_registration.rs` into route-registration and DNS/TLS policy
  submodules
- split `exec_command.rs` into surface selection and transport dispatch
  submodules
- evaluate `container_runtime_prep.rs`, `gateway_command.rs`,
  `script_command.rs`, and `bootstrap_command.rs` for similar splits
- preserve the public API of each command module
- run `cargo test` after each split to catch regressions early

## Non-Goals

- changing command behavior or CLI surface
- extracting modules into new crates
- rewriting logic (pure move and re-export only)

## Exit Condition

This milestone is complete when:

- no runner module exceeds ~800 lines
- all tests pass
- the public command API is unchanged

## Progress

Partially started:
- `gateway_registration.rs` already converted to directory module during g03.025
- Identified clear split points for remaining files:
  - `deploy_command.rs`: model (lines 62-443), render export (444-603), railway export (604-1020), types (1021-1340)
  - `bootstrap_command.rs`: deps sync (lines 607-851) is an isolated subsystem
  - `exec_command.rs`: surface selection vs transport dispatch

## Next Task

Milestone complete. No further action required on this roadmap item.
