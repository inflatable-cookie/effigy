# 244 Decide Built-In Tasks Extraction Shape

Status: archived
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

Decided 2026-04-17 after a function-level coupling sweep across
`src/runner/builtin/**` (10,096 lines total — larger than the 9.5k
estimate) and the adjacent `src/runner/scan/**` (4,928 lines).

### 1. Scope partition — single `effigy-builtin` crate

The 11 tasks share enough real helpers (`arg_parser`, `support`,
`response`, `doc_render`, `command_spec`, `help_text`, table-driven
`registry`) that splitting into per-task or per-cluster crates would
demand a shared support crate plus a registry split for no compile-time
win today. `test` (~2.2k) and `scan` (~1.9k) are the heavy clusters;
their external deps (`effigy-managed::run_spec` for test; `runner/scan`
for scan) are clean cross-crate deps, not justification for separate
builtin crates. Revisit cluster splits only if compile-unit size bites.

### 2. Crate name — `effigy-builtin`

Single-crate shape; name matches domain; no collisions in the workspace.

### 3. Prerequisite ordering — sequence after six relocations plus the scan extraction

Builtin already imports `effigy_routing::*` and `effigy_manifest::*`
directly (no routing callback inversion needed — 245/246 lesson
applies). What it still reaches for inside the runner that must move
first or sit on the runner edge via a thin callback:

- **Runner-side utility relocations (card `248`)** — small, independent
  moves bundled into one implement card:
  - `runner::util::{shell_quote, parse_dotenv_entries, normalize_builtin_test_suite}`
    → `effigy-tasks` or `effigy-core`.
  - `runner::tooling::vitest_command_for_js_package_manager` → likely
    `effigy-tasks`.
  - `runner::model::constants::{BUILTIN_TASKS, DEFAULT_BUILTIN_TEST_MAX_PARALLEL}`
    → `effigy-tasks` (or inline into the new crate if no other caller).
  - `src/data_loading::{parse_json, parse_toml, read_utf8}` →
    `effigy-core`.
  - `runner::render::encode_json` → `effigy-ui` or a shared json helper.
  - Invert `runner::deferred_builtins_for_root` so `builtin/support.rs`
    takes the deferred-builtins list as an argument rather than
    reaching up into the runner module.
  - Resolve `crate::testing::detect_test_runner_plans` —
    relocate if it belongs in a crate, or invert if app-side.
- **`effigy-scan` extraction (cards `247` decide / `249` implement)** —
  builtin/scan is a thin orchestrator over `runner/scan/{execution,
  options, render, model}`, not a duplicate; builtin must depend on
  the extracted scan crate or it will re-reach into the runner.

### 4. `builtin/scan` vs `runner/scan` — consume the extracted scan crate

The sweep resolved this: builtin/scan is orchestration (request parse →
mode dispatch → response envelope → ~1,857 lines), runner/scan is the
engine (~4,928 lines). Zero type duplication. Extract `runner/scan/**`
into `effigy-scan` as a prerequisite (opens as a decide card `247` + an
implement card `249`). Builtin then consumes it.

### 5. `builtin/arg_parser` — defer, travels with `effigy-builtin`

116-line internal cursor over `&[String]` with deps only on
`RunnerError` + `effigy_cli::TaskInvocation`. Does **not** duplicate
`effigy-cli` (CLI parses the outer `TaskInvocation`; `BuiltinArgParser`
parses the inner args inside each builtin). Self-contained — moves into
the new crate as-is and switches `RunnerError` to `BuiltinError`.

### 6. Error boundary — introduce `BuiltinError` with `From` impl

Job-8 pattern, matches `RoutingError`. Every builtin file currently
imports `RunnerError` but uses only four constructor helpers
(`task_invocation`, `task_invocation_failed_{read,parse,write,render}`)
and four variants (`TaskInvocation`, `TaskManifestCompose`,
`BuiltinTestNonZero`, `BuiltinScanNonZero`) plus the `Task*` family
surfaced by routing calls that bubble through. `BuiltinError` covers
the builtin-owned variants; routing/task errors continue to surface
through their own enums. `From<BuiltinError> for RunnerError` lives in
`src/runner/error.rs`, matching the runner-edge adapter pattern.

### 7. Consumer adapter — direct migration, no shim

242 / second-sweep lesson. Two inbound call sites only:
`execute/selection/fallback.rs::try_run_builtin_task` and
`runner/mod.rs`'s registry lookup. Both migrate directly to
`effigy_builtin::*` at extraction time. No `src/runner/builtin.rs`
re-export shim.

### 8. Hidden coupling sweep — recorded

Coupling findings beyond the top-level list:

- `test/**` heavily self-references `crate::runner::builtin::test::planning::*`
  — fine inside a single crate, costly if test were a separate crate
  (confirms decision 1).
- `scan/request/parser.rs` uses `builtin::arg_parser` — arg_parser
  travels with scan orchestration inside `effigy-builtin` (confirms
  decision 5).
- `migrate/io.rs` reaches `crate::data_loading::{parse_json, parse_toml,
  read_utf8}` — captured as a card `248` relocation.
- `test/planning/resolve/plan_resolution.rs` reaches
  `crate::testing::detect_test_runner_plans` — needs resolution in
  card `248` (relocate vs invert).
- `support.rs` reaches `crate::runner::deferred_builtins_for_root`
  from within the help surface — requires call-site inversion in
  card `248`.
- `config/output.rs` has fully-qualified `crate::runner::manifest::*`
  references at lines 281 and 412 in addition to top-of-file imports
  — grep for these during the 249 migration.
- Builtin does **not** reach into `runner::locking`, `runner::deferral`,
  `runner::command_context`, `runner::cache`, or `runner::execute` —
  none of those are hazards for this extraction.
- Registry is table-driven (`BUILTIN_REGISTRY: [BuiltinRegistryEntry; 13]`
  → `BuiltinDispatch` → match), which means a single-crate move keeps
  dispatch intact; a cluster split would require registry-split work
  (confirms decision 1).

## Next Task

Open three follow-up cards in one planning batch:

- [`247-decide-effigy-scan-extraction-shape.md`](./247-decide-effigy-scan-extraction-shape.md)
  (decide, ready) — prerequisite: scan is ~4.9k lines and needs its
  own decide pass before any implement card opens. Gates `249` and
  the full `effigy-builtin` extraction.
- [`248-implement-runner-utility-prerequisites-for-effigy-builtin.md`](./248-implement-runner-utility-prerequisites-for-effigy-builtin.md)
  (implement, ready — independent of `247`) — relocate the six
  runner-side utilities + the `deferred_builtins_for_root` inversion
  + the `testing::detect_test_runner_plans` resolution.
- [`249-implement-effigy-scan-extraction.md`](./249-implement-effigy-scan-extraction.md)
  (implement, queued behind `247`) — once `247` decides the scan
  shape, this card does the crate move.

Card `250` (implement `effigy-builtin` extraction) will open once
`247`, `248`, and `249` are all complete. It is not yet drafted.
