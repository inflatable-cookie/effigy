# 243 Decide Task Routing Core Extraction Shape

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Pin the shape of the task-routing-core extraction before any code moves.
The 010 lane named "task routing core (~6k lines)" as one of the two
remaining bounded batches after `246`, but the label bundles several
loosely-related subsystems (routing proper, scan, locking, deferral) and
the coupling reality needs a decision before an implement card can be
opened.

## Context

The "routing core" label covers ~6k lines, but only a minority of that
is routing in the strict sense:

| Subsystem | Path | Lines | Role |
|---|---|---|---|
| Catalog discovery | `src/runner/catalog/discovery.rs` | ~150 | enumerate manifest paths |
| Catalog selection | `src/runner/catalog/selection/**` | ~320 | choose catalog + task from selector |
| Catalog shim | `src/runner/catalog.rs` | ~26 | module root / re-export |
| Task scan | `src/runner/scan/**` | ~4927 | manifest parsing, scan execution, render |
| Locking | `src/runner/locking/**` | ~410 | task execution state coordination |
| Deferral | `src/runner/deferral/**` | ~391 | deferred execution + tracing |

Inbound callers for the routing surface proper (`select_catalog_and_task`
plus discovery helpers):

- `builtin/mod.rs`, `builtin/completion/**`, `builtin/cache/**`
- `execute/preflight/**`, `execute/selection.rs`
- `doctor/explain.rs`, `doctor/references.rs`
- `tasks_command.rs`, `tasks_probe.rs`
- `demo_command.rs` (via `TaskResolverFn` callback post-`245`)

Inbound callers for `scan`:

- `builtin/scan/**` (re-uses scan constants, models, execution)
- `tasks_listing/**` (consumes scan output)

Inbound callers for `locking` / `deferral`:

- `execute/pipeline/**`
- internal runner entry flow

Known coupling surprises (informed by the `238` post-mortem — the
`240`→`241` discovery pattern):

- `managed` already inverted its dependency on routing via
  `TaskResolverFn` (card `245`). Routing extracting out is now callback-free
  from the managed side.
- `builtin/scan/**` duplicates portions of `runner/scan/**`. Extracting
  scan as a crate means builtin either depends on that crate or
  `runner/scan` retains a shared surface that both crates import.
- `builtin/test/planning/**` does not touch routing but does touch
  `LoadedCatalog` (already in `effigy-manifest` post-`239`) and run-spec
  rendering (already in `effigy-managed` post-`246`).
- `scan` owns its own test module (`scan/tests.rs`) and internal
  redistribution will follow the same pattern as `246`.

## In Scope

Decide the following and record the answers in this card's `Decision`
section. Leave the section pending until the decision is made.

1. **Scope partition** — does this extraction cover only the routing
   surface proper (`catalog/**`, ~500 lines), or does it take scan
   (`scan/**`, ~4.9k) and/or locking + deferral with it? The "~6k lines"
   label is a convenience, not a coupling claim.
2. **Crate name and boundary** — one crate covering the chosen scope
   (candidates: `effigy-routing`, `effigy-catalog`, `effigy-scan`), or
   split into multiple crates (e.g. `effigy-routing` + `effigy-scan`,
   locking + deferral staying in the runner)? Consider whether folding
   routing into `effigy-tasks` or `effigy-manifest` is coherent — both
   already own adjacent concepts.
3. **Builtin/scan coupling** — `builtin/scan/**` re-implements parts of
   `runner/scan/**`. If scan extracts:
   - does builtin also migrate (out of scope here, but the decision
     shapes the dependency direction)?
   - does scan expose a narrow reusable surface (parsing, models) and
     keep execution-layer internals?
   - does builtin depend on the new crate directly, or does its own
     extraction (card `244`) consume the surface?
4. **Locking + deferral placement** — these are coupled to task execution
   state, not routing per se. Move with the extraction, stay in the
   runner, or extract separately as a later batch?
5. **Error boundary** — new `RoutingError` (or `ScanError` / `CatalogError`)
   with `From` impl in the runner, matching the job-8 pattern from
   `effigy-process`, `effigy-ui`, `effigy-managed`.
6. **Prerequisite order** — function-level grep through the chosen scope
   to surface coupling the top-level grep misses (the `238`→`241` lesson).
   If the sweep reveals hidden dependencies, insert a prerequisite card
   (utility relocates, callback inversions) before the implement card.
7. **Scope shape** — wholesale move in one implement card, or split into
   prerequisite + implement cards (following the `239`+`240` precedent)?

## Out Of Scope

- Actually moving code — this card is decision-only.
- Extraction of built-in tasks (that is card `244`).
- Reshaping `RunnerError`.
- Any pause-boundary decision for the 010 lane — that follows this
  extraction, not precedes it.

## Acceptance Criteria

- Each of the seven decisions above is answered explicitly in this card's
  `Decision` section.
- A function-level coupling sweep (not just top-level grep) is recorded,
  per the `238` post-mortem lesson.
- The 010 lane doc gains a `247 —` checkpoint naming the chosen shape.
- The next ready card is opened with the chosen scope (implement card, or
  a prerequisite card if coupling review surfaces hidden dependencies).

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

A function-level coupling sweep across the four subsystems informed the
answers. Key sweep findings:

- `scan/**` (4,928 lines) imports zero from catalog/locking/deferral,
  uses only `RunnerError::task_invocation(...)` for errors, and is
  consumed one-way by `builtin/scan/**` (1,857 lines of CLI adapter).
- `locking/**` (410 lines) is self-contained — only stdlib +
  `RunnerError` variants `TaskLockIo`, `TaskLockConflict`, plus one `Ui`
  variant for encode failures. Zero coupling to routing.
- `deferral/**` (391 lines) owns its own concerns (composer-home cache,
  trace rendering) but `policy.rs` pattern-matches the catalog-owned
  `RunnerError` variants `TaskNotFoundAny`, `TaskCatalogPrefixNotFound`,
  `TaskNotFound`. Downstream of catalog errors, not catalog logic.
- `catalog/**` (~500 lines) plus `runner/manifest.rs::load_task_manifest`
  plus `model/constants::TASK_MANIFEST_FILE` form the actual routing
  surface. Seven `RunnerError` variants (`TaskCatalogsMissing`,
  `TaskCatalogReadDir`, `TaskCatalogAliasConflict`,
  `TaskCatalogPrefixNotFound`, `TaskNotFound`, `TaskNotFoundAny`,
  `TaskAmbiguous`) are produced only here.
- Managed ↔ routing coupling is callback-only via
  `effigy_manifest::TaskResolverFn`. Card `245` already sealed this.
- `effigy-catalog` crate name is taken by the unrelated container
  service-catalog (compose assembly). Must not collide.

**Decisions:**

1. **Scope partition — catalog only (~500 lines).** Scan, locking,
   deferral do not travel with this extraction. Scan is 10× the size of
   routing proper with its own error semantics; bundling inflates the
   extraction for no coupling reason. Locking and deferral stay in the
   runner; each becomes its own future decide card.
2. **Crate name — `effigy-routing`** (new workspace crate).
   `effigy-catalog` is taken. Folding into `effigy-manifest` was
   considered — manifest owns `LoadedCatalog`, `TaskSelection`,
   `TaskResolverFn` — but rejected because routing is operational
   (filesystem discovery, selector matching) while manifest is a
   data/config layer. New crate matches the extraction-per-concern
   pattern (effigy-process, effigy-ui, effigy-managed).
3. **Builtin/scan coupling — deferred.** Scan is not in this extraction,
   so the `builtin/scan → runner/scan` dependency direction question
   belongs to a future scan decide card.
4. **Locking + deferral placement — stay in runner.** Deferral's
   pattern-matching on catalog error variants continues to work against
   `RunnerError` (which receives them via `From<RoutingError>`), so
   deferral need not depend on the new crate. Locking has no coupling to
   routing at all.
5. **Error boundary — new `RoutingError` enum with `From` impl.** Job-8
   pattern. Moves the seven catalog-owned variants into
   `effigy_routing::RoutingError`. `impl From<RoutingError> for
   RunnerError` reproduces the same variant shapes so deferral's
   pattern-matching keeps working without any deferral changes.
6. **Prerequisite order — one prerequisite card.** Sweep flagged two
   cleanups that belong before the crate move:
   - Introduce `RoutingError` inside the runner (same crate), route the
     seven variants through it via `From`, update producers. No crate
     move yet.
   - Consolidate `load_task_manifest` + `TASK_MANIFEST_FILE` into a
     shape the new crate can own directly. Today both
     `catalog/discovery.rs` and `scan/options/loading/common.rs` reach
     into `runner/manifest.rs` for these; the prereq relocates the
     catalog-side consumer so scan's parallel usage isn't accidentally
     dragged into routing's extraction.

   `task_lock_scope` relocation from `runner/manifest.rs` (also
   sweep-flagged) belongs to the future locking extraction, not this
   one.

7. **Scope shape — two cards: prerequisite + implement.** Follows the
   `239`+`240` precedent. Prerequisite card does the error-boundary
   introduction and catalog-loading glue consolidation entirely inside
   the runner. Implement card moves `catalog/**` plus the consolidated
   glue into `effigy-routing` and updates ~12 caller files.

## Next Task

Open the prerequisite card:
[`245-implement-routing-error-boundary-and-catalog-loading-consolidation.md`](./245-implement-routing-error-boundary-and-catalog-loading-consolidation.md)
— introduce `RoutingError`, move the seven catalog-owned variants behind
it with `From<RoutingError> for RunnerError`, consolidate the
`load_task_manifest` + `TASK_MANIFEST_FILE` catalog-loading glue. No
crate move.

Then open the implement card:
[`246-implement-effigy-routing-extraction.md`](./246-implement-effigy-routing-extraction.md)
— queued behind `245`. Moves `catalog/**` plus the consolidated glue
plus `RoutingError` into the new `effigy-routing` crate; migrates ~12
caller files to the new import path.
