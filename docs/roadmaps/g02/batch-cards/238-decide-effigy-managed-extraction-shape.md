# 238 Decide Effigy Managed Extraction Shape

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Pin the shape of the managed task orchestration extraction before any code
moves. The 010 lane named `src/runner/managed/**` as the next honest bounded
batch after the TUI multiprocess extraction (`241`), but the coupling
reality needs a decision before an implement card can be opened.

## Context

`src/runner/managed/**` is 37 files / ~4.1k lines (including tests). Inbound
dependency sweep shows the tree imports three runner-local surfaces:

- `crate::runner::error::RunnerError` — runner-owned error type, used in
  ~20+ files in the managed tree
- `crate::runner::model::catalog::LoadedCatalog` — runner-local
  (`pub(in crate::runner)`), owns a `TaskManifest`; used across command,
  run_spec, references
- `crate::runner::manifest::*` — mostly pass-through re-exports of
  `effigy_manifest::*`; trivial to rewrite to direct `effigy-manifest`
  imports
- `crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions}` — already
  re-exports from `effigy-tui` since `241`; managed's `runtime.rs` can
  depend on `effigy-tui` directly

Outbound callers into managed (three, all via `crate::runner::managed::*`):

- `src/runner/demo_command.rs`
- `src/runner/execute/pipeline/{command,managed}.rs`
- `src/runner/builtin/test/{execution,planning}/...`

## In Scope

Decide the following and record the answers in this card and the 010 lane
doc:

1. **Crate shape** — new `effigy-managed` crate, or fold managed into the
   existing `effigy-tasks` crate (currently parsing-only)?
2. **`RunnerError` boundary** — does the new crate own its own error type
   (managed-local error, with a `From` impl in the runner), or does
   `RunnerError` move to a shared crate first?
3. **`LoadedCatalog` boundary** — does managed accept a data-only input
   (borrowed fields / a trait) so it never imports `LoadedCatalog`, or does
   `LoadedCatalog` move to a shared crate first?
4. **Scope shape** — wholesale move of `managed/**` in one batch, or carve
   the self-contained subtrees first (`scheduler/graph`,
   `run_spec/sequence/env_resolution`) while leaving `runtime.rs` /
   `references.rs` / `command.rs` behind for a later sub-batch?
5. **Consumer adapter** — keep `src/runner/managed.rs` as a thin re-export
   shim (matching the `effigy-tui` and `effigy-process` pattern), or rewire
   consumers to the new crate path directly?

## Out Of Scope

- actually moving code — this card is decision-only
- extending the extraction to routing core (catalog/scan/locking/deferral)
  or built-in tasks; those are separate queued batches
- reshaping `RunnerError` beyond what's needed to unblock the decision

## Acceptance Criteria

- each of the five decisions above is recorded explicitly in this card's
  `Decision` section
- the 010 lane doc is updated with a `242 —` checkpoint naming the chosen
  shape
- the next ready card is opened with the chosen scope (implement card, or
  a prerequisite card if `RunnerError` / `LoadedCatalog` need to move first)

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

Coupling review (grep-anchored, not guessed):

- `RunnerError` is imported in ~20 managed files; `LoadedCatalog` in 11
  managed files and 67 non-managed runner files (78 total call sites).
- Managed also owns two pure-data shapes that currently live in
  `src/runner/model/managed.rs`: `ManagedProcessSpec` and
  `ManagedTaskPlan`. Both are `pub(in crate::runner)` and have no deps
  beyond `PathBuf`.
- `src/runner/model/catalog.rs` re-exports `effigy_tasks::{TaskSelector,
  TaskRuntimeArgs, CatalogSelectionMode}` and owns `LoadedCatalog`,
  `TaskSelection`, `DeferredCommand`. `LoadedCatalog` wraps a
  `TaskManifest` from `effigy-manifest`.
- `crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions}` already
  re-exports from `effigy-tui` since `241`; managed's `runtime.rs` can
  take the direct `effigy-tui` dep with no plumbing.
- `crate::runner::manifest::*` is almost entirely pass-through
  re-exports of `effigy_manifest::*`; rewriting managed's imports to
  hit `effigy-manifest` directly is mechanical.

Decisions:

1. **Crate shape** — new `effigy-managed` crate.
   - `effigy-tasks` is intentionally thin (parsing + contract shapes, no
     manifest dep). Folding ~4.1k lines of runtime orchestration into it
     would change its character and force a `effigy-manifest` dep on a
     crate that deliberately avoids one.
   - Matches the established pattern (`effigy-process`, `effigy-ui`,
     `effigy-tui`, `effigy-manifest`).

2. **`RunnerError` boundary** — managed owns `ManagedError`.
   - Pattern precedent: `effigy-process::ProcessManagerError` and
     `effigy-ui` errors both have `From<DomainError> for RunnerError`
     impls in the runner (the job-8 pattern already recorded in this
     lane doc at `239`).
   - Avoids a massive prerequisite relocate of `RunnerError` itself.

3. **`LoadedCatalog` boundary** — relocate into `effigy-manifest` as a
   prerequisite sub-batch, _before_ managed extracts.
   - Semantic fit: `LoadedCatalog` wraps a `TaskManifest`; it is a
     manifest-loading concept, not a managed-runtime concept.
   - Rejected: moving it into `effigy-managed` would force 67
     non-managed runner files (`tasks_listing/**`, `tasks_view/**`,
     `catalog/**`, `scan/**`, ...) to import from a crate named
     "managed" when they have nothing to do with managed execution.
   - Rejected: trait-view adapter would require rewriting every managed
     function signature and every caller in the runner. Much larger
     churn than a relocate.
   - `TaskSelection` and `DeferredCommand` travel with `LoadedCatalog`
     (same file, same concern).

4. **Scope shape** — split into two implement cards.
   - `239`: prerequisite relocate of `LoadedCatalog` / `TaskSelection` /
     `DeferredCommand` from `src/runner/model/catalog.rs` into
     `effigy-manifest`. Mechanical 67+11 = 78 import rewrites; leaves
     `src/runner/model/catalog.rs` as a re-export shim.
   - `240`: the managed extraction proper — move `src/runner/managed/**`
     and the two `runner::model::managed` shapes into a new
     `effigy-managed` crate; introduce `ManagedError` with a runner-side
     `From` impl; managed depends on `effigy-manifest` (for
     `LoadedCatalog` + runtime manifest types), `effigy-process`,
     `effigy-ui`, `effigy-tui`, `effigy-tasks`.
   - Wholesale-in-one-card was rejected: even with mechanical work, the
     combined change is ~4k lines moved + 78 import rewrites, which
     exceeds the "bounded batch" envelope the lane enforces.
   - Carve-subtrees-first was rejected: would leave half-extracted
     positions. Only `scheduler/graph` and `scheduler.rs` are truly
     self-contained; everything else reaches `LoadedCatalog`.

5. **Consumer adapter** — preserve thin re-export shims at both layers
   during the transition.
   - `src/runner/model/catalog.rs` stays as a thin shim after `239`
     (re-exports from `effigy_manifest`).
   - `src/runner/managed.rs` stays as a thin shim after `240`
     (re-exports from `effigy_managed`).
   - Matches the `effigy-tui` pattern established by `241`.
   - Rewiring the three external consumers
     (`demo_command`, `execute/pipeline/*`, `builtin/test/*`) to hit the
     crates directly is a follow-up sweep, not part of either card.

## Post-Mortem Addendum (after first `240` attempt)

The `RunnerError | LoadedCatalog | ManagedTaskPlan` grep sweep that
anchored the five decisions missed a whole class of coupling. A
function-level grep through `src/runner/managed/**` after the first
in-flight extraction attempt surfaced ~500 lines of extra
runner-local dependencies:

- `catalog::select_catalog_and_task` (routing core — the next queued
  batch)
- `env_schema_support::resolve_catalog_env_schema` (94 LOC)
- `util::parse_task_reference_invocation` (~100 LOC)
- `util::parse_dotenv_entries`, `util::shell_quote`,
  `util::render_passthrough_args` (smaller helpers)
- `model::constants::BUILTIN_TASKS` (13-entry constant)

The consequence is that `240` as originally scoped would have to
move the whole managed tree _and_ relocate ~500 LOC of utilities
across four shared crates _and_ invert a dependency on routing
core. That exceeds the bounded-batch envelope.

Revised plan: insert card `241` as a prerequisite that handles the
utility relocates (shell → `effigy-core`, dotenv + env-schema →
`effigy-env`, reference parsing → `effigy-tasks`) and the callback
contract for `select_catalog_and_task` so managed no longer reaches
into routing core. `240` then moves only the managed tree and the
plan shapes, staying mechanical.

The five original decisions still hold — this addendum widens the
queue, not the shape.

## Next Task

Open and execute
[`239-implement-effigy-manifest-loaded-catalog-relocate.md`](./239-implement-effigy-manifest-loaded-catalog-relocate.md)
first (landed in commit `eaf6eac0`), then
[`241-implement-runner-util-prerequisites-for-managed-extraction.md`](./241-implement-runner-util-prerequisites-for-managed-extraction.md)
(ready), and finally
[`240-implement-effigy-managed-extraction.md`](./240-implement-effigy-managed-extraction.md)
(queued).
