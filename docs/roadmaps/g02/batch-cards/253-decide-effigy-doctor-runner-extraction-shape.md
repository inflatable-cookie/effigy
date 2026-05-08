# 253 Decide Effigy-Doctor-Runner Extraction Shape

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Pin the shape of the doctor-runner extraction before any code moves.
`src/runner/doctor/**` is ~4,532 lines across 65 files — the largest
bounded subsystem left inside the runner after the `effigy-builtin`
extraction. It sits alongside the existing `effigy-doctor` library
crate (705 LOC, pure report / finding / projection types), so the
naming and boundary need a deliberate decision rather than a default.

The implement card is `254`.

## Context

`src/runner/doctor/**` contents (~4,532 LOC, 65 files):

| Subsystem | Path | Files | LOC | Role |
|---|---|---:|---:|---|
| Workflow orchestrator | `doctor/workflow/**` | 6 | 779 | Phase pipeline, root resolution, manifest prep, fix application |
| Render | `doctor/render/**` | 6 | 485 | Text/JSON rendering for reports and scan output |
| Scan checks | `doctor/scan_checks/**` | 4 | 381 | Filesystem scan checks (god-files, stale suppressions, etc.) |
| Finding templates | `doctor/finding_templates/**` | 4 | 339 | Finding factory functions |
| Explain | `doctor/explain/**` | 3 | 377 | `--explain <check>` rendering and analysis |
| Manifest | `doctor/manifest/**` | 3 | 324 | Manifest discovery, parsing, schema validation |
| Checks entry | `doctor/checks/**` | 5 | 348 | Check definitions, catalog/test-runner checks, executor |
| Report model | `doctor/report/**` | 2 | 153 | `DoctorState`, `ManifestSnapshot` |
| Health | `doctor/health/**` | 4 | 172 | Health task invocation / summarization |
| Task graph | `doctor/task_graph/**` | 1 | 69 | Task DAG analysis |
| Contracts metadata | `doctor/contracts/**` | 1 | 95 | Explain contracts metadata |
| Root modules | (various) | 24 | 850 | `command.rs`, `environment.rs`, `references.rs`, `progress.rs`, individual scan-check wrappers, etc. |

### Residual runner reach-ins (post-card-252 survey)

Only four distinct runner-internal imports across 37 files:

1. **`crate::runner::error::RunnerError`** — every function returns
   `Result<T, RunnerError>`. Boundary lift goes here.
2. **`crate::runner::command_context::current_working_dir()`** — 2
   call sites (`workflow.rs`, `explain.rs`). Pure ~5-line helper
   around `std::env::current_dir()` with error mapping.
3. **`crate::runner::manifest::config_sections::ManifestJsPackageManager`**
   — 1 call site (`report/snapshot.rs`). Type actually lives in
   `effigy-manifest`; imported through runner's `manifest.rs`
   re-export for historical reasons.
4. **No reaches to `deferral`, `locking`, `util`, `cache`, or
   `execute`.**

### Test coverage

- Inline `#[test]`s under `src/runner/doctor/**`: ~30 tests, est. 200 LOC
- `src/tests/runner_tests/tasks_and_doctor_command_tests/`: 913 LOC
- `src/tests/runner_tests/doctor_text_output_tests/`: 320 LOC

Inline tests cover module logic (`task_graph`, `workflow`,
`checks`); integration tests cover command-level contracts.

## Decision

### Crate name: **fold into existing `effigy-doctor`**

No new crate. The existing `effigy-doctor` is already
doctor-shaped; the runner-side orchestration naturally belongs in
the same crate alongside the report and finding types it already
owns. This matches the pattern set by `effigy-tasks`,
`effigy-release`, `effigy-managed`, etc. (domain crates own both
types and orchestration).

Final crate size: 705 LOC → ~5,200 LOC. Still modest for a domain
crate. Alternative (`effigy-doctor-runner`) would create an
awkward split where half the doctor surface lives in one crate
and half in another.

### Error boundary: `DoctorError` enum in `effigy-doctor`

New `DoctorError` enum mirroring the `BuiltinError`/`ScanError`
pattern:

```rust
pub enum DoctorError {
    DoctorNonZero { error_count: usize, rendered: String },
    TaskInvocation(String),
    Ui(String),
    Manifest(ManifestError),
    Scan(ScanError),
}
```

Plus helper constructors (`task_invocation_failed_{read,parse,write,render}`)
matching the `BuiltinError` pattern. `impl From<DoctorError> for
RunnerError` at the runner's edge lifts each variant one-for-one.

Every runner-side `Result<T, RunnerError>` inside `doctor/**`
becomes `Result<T, DoctorError>` post-move. The runner's
`doctor::run_doctor` wrapper (which currently returns
`Result<String, RunnerError>`) stays at the runner boundary with
a single `.map_err(RunnerError::from)` lift.

### Port surface: **none**

No port traits. Doctor depends directly on:

- `effigy-manifest` (for `ManifestJsPackageManager`, manifest
  loading, schema types — mostly already accessible)
- `effigy-scan` (for the scan engine that feeds scan_checks)
- `effigy-core` (path/IO helpers, widgets for rendering)
- `effigy-env` (env schema when applicable)
- `effigy-ui` (renderers)
- `effigy-tasks` (task selector, task references)
- `effigy-routing` (catalog discovery)

No runner-internal dependency. `current_working_dir()` either
gets duplicated into the crate (~5 LOC, one helper) or is already
available through `effigy-core`. Survey suggests either works;
implement card picks the cleaner of the two.

### Split shape: **single crate, no cluster**

4.5k LOC is well below the threshold where a split pays off
(`effigy-builtin` was 10k and stayed a single crate under card
250). No natural sub-domain boundary inside doctor that would
warrant separation.

### Prerequisites: **none**

Card 252 already retired `env_schema_support.rs` and
flattened `doctor/run/workflow/` → `doctor/workflow/` plus
`doctor/run/check_registry/` → `doctor/checks/`. With those
trims in place, card 254 is a mechanical crate move with no
pre-requisite relocations needed.

## Implement Card (254) Scope Anchor

Card 254 executes:

1. Grow the existing `effigy-doctor/Cargo.toml` with the new
   domain deps (`effigy-scan`, `effigy-core`, `effigy-ui`,
   `effigy-env`, `effigy-tasks`, `effigy-routing`,
   `effigy-manifest` — most already present or trivial to add).
2. Move every file under `src/runner/doctor/**` into
   `crates/effigy-doctor/src/`. Preserve the post-card-252
   directory shape.
3. Introduce `DoctorError` in `effigy-doctor/src/error.rs` (or
   extend the existing finding/report modules).
4. Rewrite every `Result<T, RunnerError>` inside the moved tree
   to `Result<T, DoctorError>`.
5. Add `impl From<DoctorError> for RunnerError` in
   `src/runner/error.rs` (mirrors the `From<BuiltinError>` lift
   pattern).
6. Runner-side: `src/runner/doctor.rs` shrinks to a single-function
   shim — `pub(crate) fn run_doctor(args) -> Result<String, RunnerError>`
   that calls `effigy_doctor::run_doctor(args).map_err(RunnerError::from)`.
   Or the runner drops its `doctor.rs` entirely and
   `entrypoints/dispatch.rs` calls `effigy_doctor::run_doctor`
   directly.
7. Inline or replace `crate::runner::command_context::current_working_dir()`
   at the 2 call sites (inline `std::env::current_dir()` plus
   error mapping, or import from `effigy-core` if the helper
   already lives there).
8. Flip `ManifestJsPackageManager` import at
   `report/snapshot.rs` to `effigy_manifest::config_sections::*`
   directly.
9. Migrate any inline tests that use runner-internal types to
   their new home.

## Decision Checklist

- [x] Crate name: fold into existing `effigy-doctor`
- [x] `DoctorError` variants: `DoctorNonZero`, `TaskInvocation`,
      `Ui`, `Manifest(ManifestError)`, `Scan(ScanError)`
- [x] Port traits: none
- [x] Split shape: single crate
- [x] Prerequisite cards: none
- [x] Implement card (254) scoped in this card

## Next Task

Execute card `254` — the mechanical extraction per this decision.
