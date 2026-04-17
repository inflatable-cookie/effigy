# 244 Decide Built-In Tasks Extraction Shape

Status: ready
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Pin the shape of the built-in tasks extraction before any code moves.
The 010 lane named "built-in tasks (~9.5k lines)" as one of the two
remaining bounded batches after `246`. Unlike the managed extraction
(one coherent runtime) and the routing extraction (one coherent domain),
builtin tasks are heterogeneous: 11 distinct tasks covering init, migrate,
unlock, watch, doctor, scan, cache, completion, tasks, config, test.
Whether to extract wholesale, per-task, or per-cohesive-cluster is a
real shape call.

## Context

The builtin subsystem lives in `src/runner/builtin/**`, ~10k lines total.
Breakdown:

| Subsystem | Path | Lines |
|---|---|---|
| Dispatcher / entry | `builtin/mod.rs`, `registry.rs`, `command_spec.rs`, `response.rs`, `support.rs` | ~530 |
| Small task bodies | `builtin/{config,tasks,help,init,migrate,unlock,watch,doctor,scan,completion,cache,test,text_doc}.rs` entry files | ~400 |
| `init/` subtree | `builtin/init/**` | ~102 |
| `config/` subtree | `builtin/config/**` | ~175 |
| `completion/` subtree | `builtin/completion/**` | ~550 |
| `cache/` subtree | `builtin/cache/**` | ~260 |
| `test/` subtree | `builtin/test/**` | ~1800 |
| `scan/` subtree | `builtin/scan/**` | ~1800 |
| `arg_parser/` | `builtin/arg_parser/**` | ~180 |

Inbound callers:

- `execute/selection/fallback.rs` → `try_run_builtin_task` (dispatcher)
- `runner/mod.rs` → registry lookup

Outbound coupling (what builtin reaches into):

- `effigy-managed::run_spec::{render_run_step_sequence, resolve_run_step_env, wrap_command_with_env}` (test planning)
- `runner/scan/**` (builtin/scan re-uses constants, models, execution)
- `effigy-manifest::LoadedCatalog` (test planning, completion, scan)
- `effigy-tasks::TaskRuntimeArgs`, `TaskSelector`
- `effigy-ui` (render helpers across most tasks)
- `effigy-process` (test execution)
- Routing core (`select_catalog_and_task`, `discover_catalogs`) — several
  tasks call it directly; unlike managed, builtin has not yet been
  inverted via callback.

Heterogeneity matters: the 11 tasks have very different concerns.

- `init`, `migrate`, `unlock` — simple scaffold / state operations, thin
- `config`, `tasks`, `help` — projection over manifest / catalog data
- `doctor` — reaches into the doctor crate; already thin
- `completion`, `cache` — self-contained, moderate size
- `scan`, `test` — large, reach into multiple shared surfaces
- `text_doc` — documentation helper

Order-of-extraction concerns:

- Builtin/scan duplicates runner/scan. If routing core extracts first
  (card `243`) and scan becomes a crate, builtin/scan's migration
  becomes a re-target rather than a full move.
- Builtin/test reaches into `effigy-managed::run_spec`. This is already
  a clean cross-crate dep; no shim needed.
- Builtin calls `select_catalog_and_task` directly. Unlike managed
  (inverted at `245`), builtin has no callback boundary. If builtin
  extracts before routing, either the call site moves to the crate (which
  then depends on routing), or a callback inversion is added first.

## In Scope

Decide the following and record the answers in this card's `Decision`
section. Leave the section pending until the decision is made.

1. **Scope partition** — wholesale extraction of all 11 tasks into a
   single `effigy-builtin` crate, or carve cohesive clusters (e.g.
   `effigy-builtin-test`, `effigy-builtin-scan`, `effigy-builtin-core`
   for the rest)? Consider whether each task's coupling surface justifies
   its own crate boundary or whether the overhead outweighs the gain.
2. **Crate name(s)** — `effigy-builtin` for the whole surface vs
   per-cluster names vs per-task names. Dispatcher / registry placement
   follows from this choice.
3. **Prerequisite ordering vs routing** — does this batch sequence after
   `243` (routing core), so builtin can depend on the extracted
   routing/scan crates without reaching into the runner? Or does it
   sequence first, with callback inversions inserted as prerequisites?
4. **`builtin/scan` vs `runner/scan`** — resolve the duplication. Options:
   (a) builtin/scan consumes the extracted scan crate (requires `243`
   first); (b) the shared surface is relocated into `effigy-tasks` or
   `effigy-manifest` as a prerequisite; (c) the duplication is accepted
   and builtin carries its own copy.
5. **`builtin/arg_parser`** — the ~180-line custom arg parser duplicates
   logic similar to the CLI's own parser. Defer (leave in the builtin
   crate), consolidate into CLI parsing first as prerequisite, or
   extract into a shared parsing crate?
6. **Error boundary** — one `BuiltinError` with `From` impl in the runner
   (matching the job-8 pattern), or surface builtin errors through the
   existing runner error type without introducing a new domain error?
7. **Consumer adapter** — does the runner keep a thin re-export shim at
   `src/runner/builtin.rs` during transition (matching the `246` pattern
   before card `242` cleanup), or does the dispatcher migrate call sites
   directly?
8. **Function-level coupling sweep** — before any implement card opens,
   run a function-level grep through `builtin/**` to surface hidden
   coupling the top-level sweep misses (the `238`→`241` lesson). Record
   findings. Insert prerequisite cards for any surfaces that need to
   move first.

## Out Of Scope

- Actually moving code — this card is decision-only.
- Extraction of routing core (that is card `243`).
- Reshaping `RunnerError`.
- Any pause-boundary decision for the 010 lane — that follows this
  extraction, not precedes it.

## Acceptance Criteria

- Each of the eight decisions above is answered explicitly in this
  card's `Decision` section.
- A function-level coupling sweep is recorded.
- The 010 lane doc gains a checkpoint naming the chosen shape.
- The next ready card is opened with the chosen scope (implement card,
  or one or more prerequisite cards).

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

_Pending — populate after coupling review._

## Next Task

_Pending — the follow-up card(s) will be named in the Decision section
once decided._
