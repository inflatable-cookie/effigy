# 252 Implement Runner-Root Shim And Directory Tidy

Status: done
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Retire the residual single-call-site shims and collapse the two worst
directory-depth offenders inside `src/runner/**`. No behavior change,
no crate moves — this is a pure tidy pass that drops the
`super::super::super::*` count from ~37 to ~10 and deletes three
zero-value re-export files.

## Context

Post-`effigy-builtin` extraction (card `250`), the runner still carries
a handful of leftover shim files and two over-nested subtrees that
exist only because earlier extractions left debris behind. None of
these warrant their own decide card; they're mechanical inlines and
renames bounded to a single commit.

### Shim inventory (to inline or delete)

| File | LOC | Status | Action |
|---|---:|---|---|
| `src/runner/manifest.rs` | 44 | pure `pub(in crate::runner) use effigy_manifest::*` re-exports | delete; flip call sites to import `effigy_manifest` directly |
| `src/runner/env_schema_support.rs` | 43 | thin adapter, one consumer (doctor) | inline into `src/runner/doctor/` at the consumer site |
| `src/runner/render.rs` | 56 | trace-render helpers, one consumer (`deferral/trace.rs`) | inline into `deferral/trace.rs` |
| `src/runner/model/constants.rs` | 18 | two deferral constants | move into `src/runner/deferral/policy.rs`; drop `model/` dir if empty |
| `src/runner/mod.rs` `deferred_builtins_*` fns | ~30 | deferral-policy helpers misplaced at runner root | move into `src/runner/deferral/` |
| `src/cli/runner_dispatch.rs` | 57 | single-hop bridge between entrypoint and `run_and_render_command` | inline into `src/cli/entrypoint.rs` |

### Directory-depth offenders

- `src/runner/doctor/run/workflow/**`, `src/runner/doctor/run/check_registry/**`
  — both sit 4 levels deep inside the runner tree. Every file
  reaches through `super::super::super::*` for sibling doctor
  helpers. Promote to `src/runner/doctor/workflow/` and
  `src/runner/doctor/checks/`, eliminating the `run/` intermediate.
- `src/runner/execute/selection/fallback/**` — 4 files buried 4
  levels deep; only `builtin.rs` reaches into anything interesting.
  Flatten to `src/runner/execute/selection/fallback.rs` (single
  file) or promote to `src/runner/execute/selection_fallback/`.

## In Scope

- ~~Delete `src/runner/manifest.rs`~~ — surveyed during execution,
  file is not a pure shim (four live helpers plus type re-exports,
  all in active use). Left in place.
- Inline `env_schema_support.rs` into its single consumer
  (`execute/pipeline/standard.rs`, not doctor as originally
  scoped). Deleted the file.
- Move `render_task_resolution_trace` into `execute/context.rs`
  (its only caller); drop `trace_renderer` alias in favor of
  `effigy_ui::text_renderer()` direct at the two sites. Deleted
  `render.rs`.
- Moved `DEFER_DEPTH_ENV`, `IMPLICIT_ROOT_DEFER_TEMPLATE`,
  `EXPLICITLY_DEFERRABLE_COMMAND_BUILTINS`, and
  `IMPLICITLY_DEFERRED_COMMAND_BUILTINS` into
  `src/runner/deferral/policy.rs`. Deleted `src/runner/model/`.
- Moved `deferred_builtins_for_root`,
  `deferred_builtins_from_catalogs`, and
  `builtin_can_be_explicitly_deferred` from `src/runner/mod.rs`
  into a new `src/runner/deferral/builtins.rs` module. CLI call
  sites continue to route through `crate::runner::*` via a
  re-export.
- Flattened `doctor/run/workflow/` → `doctor/workflow/` and
  `doctor/run/check_registry/` → `doctor/checks/`. Deleted
  `doctor/run.rs` and `doctor/run/` wrapper. Updated imports:
  `super::super::*` references inside the subtrees dropped one
  level.
- Flattened `execute/selection/fallback/` (three files) into a
  single `execute/selection/fallback.rs`. Private internals are
  now just free functions rather than submodule re-exports.
- Inlined `cli/runner_dispatch.rs` into `cli/entrypoint.rs` (the
  only caller). Public re-export in `src/lib.rs` repointed at the
  new location.

## Out Of Scope

- Any crate extraction. `doctor/` stays inside `src/` — that's card
  `254`'s job.
- Test-harness prelude flattening — card `255`.
- Any behavior change. Error types, public surfaces, and output
  shapes are identical before and after.
- Reorganizing `execute/pipeline/**` beyond the `fallback/` subtree.

## Acceptance Criteria

- `src/runner/env_schema_support.rs`, `src/runner/render.rs`,
  `src/runner/model/` all gone. `src/runner/manifest.rs` remains
  intentionally (not a pure shim).
- Grep for `super::super::super::super::` across `src/runner/`
  returns **0** matches (vs. ~12 before). Grep for
  `super::super::super::` drops to **15** matches (vs. ~37 before).
- `src/runner/mod.rs` no longer defines `deferred_builtins_*` or
  `builtin_can_be_explicitly_deferred` — they live in
  `runner/deferral/builtins.rs`, re-exported at the runner root
  for CLI call sites.
- `src/cli/runner_dispatch.rs` gone; `run_and_render_command`
  lives in `cli/entrypoint.rs`.

## Validation

- `cargo build --all-targets`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Next Task

Card `253` — decide the shape of the doctor-runner extraction
(the largest remaining subsystem inside the runner, ~4.5k LOC).
