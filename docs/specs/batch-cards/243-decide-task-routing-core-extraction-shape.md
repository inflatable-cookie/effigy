# 243 Decide Task Routing Core Extraction Shape

Status: ready
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

_Pending — populate after coupling review._

## Next Task

_Pending — the follow-up card (implement or prerequisite) will be named
in the Decision section once decided._
