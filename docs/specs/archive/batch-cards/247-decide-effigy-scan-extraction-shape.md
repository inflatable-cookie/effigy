# 247 Decide Effigy-Scan Extraction Shape

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Pin the shape of the scan extraction before any code moves. `src/runner/scan/**`
is 4,928 lines — larger than `effigy-managed` at extraction time and by far
the largest remaining bounded subsystem inside the runner. It also sits on
the critical path to `effigy-builtin` (card `244`), because `builtin/scan/**`
is a thin orchestration layer over this engine.

This card decides the scope partition, crate name, error boundary,
prerequisite ordering, and split-shape for the implement work. The implement
card is `249`.

## Context

`src/runner/scan/**` contents, ~4,928 lines across six subsystems:

| Subsystem | Path | Lines | Role |
|---|---|---|---|
| Model types | `scan/model/**` | ~541 | scan result shapes, render-format tags |
| Support helpers | `scan/support/**`, `scan/support.rs` | ~1,150 | traversal, heuristics (markers, code, generated, strings, severity), constants |
| Execution | `scan/execution/**` | ~1,080 | workspace scan, file-scans, marker scans, duplicate-block engine |
| Options / loading | `scan/options/**`, `scan/options.rs` | ~700 | manifest-backed scan options, defaults, validation, loading traits/impls |
| Render | `scan/render/**` | ~1,260 | text/JSON reports per scan category |
| Tests | `scan/tests.rs` | 197 | unit tests at subsystem root |
| Constants | `scan/constants.rs` | ~15 | generated-asset / marker constants |

Inbound callers from outside `scan/**`:

- `src/runner/builtin/scan/**` — the CLI orchestration layer for user-invoked
  `effigy scan` runs. Uses `scan::model::*`, `scan::execution::*`,
  `scan::options::*`, `scan::render::*`, `scan::support::*` — every
  subsystem.
- `src/runner/doctor/**` — individual doctor checks each run a narrow scan
  (`attention_markers`, `comment_ratio`, `duplicate_blocks`, `generated_assets`,
  `generated_in_src`, `god_files`, `stale_suppressions`, `manifest`,
  `scan_checks/mod`).
- `src/runner/builtin/watch/runtime.rs` — imports scan models.
- `src/runner/builtin/registry.rs` — references the scan task wiring.

Outbound coupling (what scan reaches into):

- `RunnerError::task_invocation(...)` — the only error surface; no
  scan-specific variant exists today.
- `crate::runner::manifest::{load_task_manifest, TaskManifest}` — from
  `options/loading/common.rs`. This is the **parallel path** kept alive
  deliberately by card `245` (catalog moved to its own `manifest_load.rs`;
  scan was left pointing at `runner/manifest.rs`).
- `crate::runner::manifest::config_sections::*` — manifest config-sections
  types used directly in several scan files.
- `crate::runner::model::constants::TASK_MANIFEST_FILE` — from
  `options/loading/common.rs`.
- `effigy-tasks` / `effigy-manifest` — via the manifest types.
- Standard library + `serde` + `toml` for the loading path.

Coupling surprises (informed by card `238` post-mortem plus the `244`
function-level sweep):

- Scan's options loading is manifest-coupled. Extracting scan as a crate
  means either:
  - the extracted crate takes a hard dep on `effigy-manifest` (reasonable,
    same pattern as `effigy-managed`), OR
  - the options-loading layer stays in the runner and the crate exposes a
    narrower engine surface.
- `runner/manifest.rs`'s `load_task_manifest` is still used by scan and by
  `builtin/migrate/io.rs`. If scan moves into a crate, that helper needs
  either (a) to move with it, (b) to be duplicated in `effigy-manifest`, or
  (c) stay in the runner and scan calls back into the runner through a
  narrow interface.
- Scan has zero coupling to catalog, locking, deferral, or execute/pipeline.
  It is downstream of manifest loading and upstream of render.
- `builtin/scan/**` (~1,857 lines) is orchestration around this engine, not
  a duplicate. After this extraction, `builtin/scan/**` will import from
  `effigy-scan` directly — confirmed by the `244` sweep.
- `doctor/**` scan checks are small per-file adapters that already call
  `scan::execution::*` directly. Their migration is mechanical — imports
  flip from `crate::runner::scan::*` to `effigy_scan::*`.

## In Scope

Decide the following and record the answers in this card's `Decision`
section. Leave the section pending until the decision is made.

1. **Scope partition** — does the extraction cover all six subsystems
   (model, support, execution, options, render, tests) in one crate, or
   does options-loading stay in the runner (so the crate has no
   `effigy-manifest` dep), or does render split out separately? The total
   is 4,928 lines; splitting costs a shared-internals crate.
2. **Crate name** — `effigy-scan` is the obvious default; check the
   workspace for collisions (there are none known). Confirm.
3. **Error boundary** — introduce `ScanError` with
   `From<ScanError> for RunnerError` (Job-8 pattern), or keep scan returning
   `RunnerError::task_invocation(...)` directly since that is the only
   variant it uses today? The `ScanError` form costs an enum and a From
   impl; the plain-RunnerError form costs the crate depending on
   `effigy` for the `task_invocation` constructor.
4. **Manifest-loading glue** — resolve the scan-side use of
   `runner/manifest.rs::load_task_manifest` and
   `model/constants::TASK_MANIFEST_FILE`. Options:
   - (a) Consolidate into a scan-owned `manifest_load.rs` inside the new
     crate, mirroring card `245` Part B for routing.
   - (b) Relocate `load_task_manifest` into `effigy-manifest` so both
     scan and the remaining `builtin/migrate/io.rs` caller consume a
     crate function.
   - (c) Keep the glue in the runner; scan's options-loading layer stays
     behind.
5. **Prerequisite ordering vs card `248`** — card `248` relocates six
   runner-side utilities that belong to `effigy-builtin`'s prereq list.
   Does scan depend on any of those same utilities? (`runner/render`,
   `runner/tooling`, `runner/util`, `runner/data_loading`, runner
   constants.) If so, sequence accordingly.
6. **Test redistribution** — `scan/tests.rs` travels with the crate.
   Confirm the test total bookkeeping (runner lib count drops by ~8; a
   new `effigy-scan` crate count appears).
7. **Scope shape — one card or split** — is a single implement card
   sufficient, or does a prerequisite card for the error-boundary or
   manifest-glue consolidation precede the move (following the
   `245`+`246` precedent)?

## Out Of Scope

- Actually moving code — this card is decision-only.
- Extraction of `builtin/scan/**` — that consumes `effigy-scan` and is
  part of card `244`'s follow-up (eventual `effigy-builtin` implement
  card, not yet drafted).
- Relocating `builtin/migrate/io.rs` or its manifest-loading use — belongs
  to card `248` or the eventual `effigy-builtin` implement card.
- Reshaping `RunnerError`.
- Any pause-boundary decision for the 010 lane.

## Acceptance Criteria

- Each of the seven decisions above is answered explicitly in this card's
  `Decision` section.
- A function-level coupling sweep is recorded (call sites, outbound deps,
  error-variant producers, manifest-loading reach).
- The 010 lane doc gains a checkpoint naming the chosen shape.
- The next ready card is opened with the chosen scope
  (card `249` — implement, or a prerequisite card if the sweep surfaces
  hidden dependencies).

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

Decided 2026-04-17 after a function-level coupling sweep across
`src/runner/scan/**` (4,928 lines, 34 files).

### 1. Scope partition — single crate covering all six subsystems

All of `model/**`, `support/**`, `execution/**`, `options/**`,
`render/**`, plus `tests.rs` and `constants.rs` move as one crate. The
subsystems are tightly internal: `execution` uses `support` + `model`;
`render` consumes `model` types (`ScanRenderFormat`, `TextRenderOptions`);
`options/loading` builds the same model types that `execution` emits.
Splitting `render` out would force duplicating `model/common.rs` across
two crates. Keeping `options/loading` in the runner fragments six
`load_root_*` / six `doctor_*` option builders from the types they
construct. 4,928 lines is comparable to `effigy-managed` at extraction
time; one compile unit is fine.

### 2. Crate name — `effigy-scan`

Pre-reserved. `crates/effigy-managed/src/lib.rs` already maps
`("scan", "effigy-scan")` in its crate-slug table. No workspace
collisions in `Cargo.toml` or `crates/*/Cargo.toml`.

### 3. Error boundary — introduce `ScanError` with `From<ScanError> for RunnerError`

Job-8 pattern. Scan produces errors at exactly 20 call sites, all
`RunnerError::task_invocation(...)`:
- `execution/workspace.rs:77`, `execution/file_scans/generated.rs:81,145`
- `support/traversal/walker.rs:21,26,72,107,114`
- `options/validation.rs:40,49,62,74,91,108,121,133,148,153,167,172`

Shape: single-variant newtype `ScanError::Invocation(String)` is
sufficient today, but a richer enum (`InvalidGlob`, `ReadFailed`,
`ValidateBounds`) is the natural first refactor inside the new crate
and can land incrementally. `From<ScanError> for RunnerError` lifts
every variant to `RunnerError::task_invocation(...)`. Avoids the
scan-depends-on-effigy cycle.

### 4. Manifest-loading glue — consume `effigy-manifest` directly

Every scan file currently reaching into `crate::runner::manifest::*`
is reaching through a pure re-export: `src/runner/manifest.rs` lines
7-14 re-export `effigy_manifest::config_sections` and
`effigy_manifest::task_runtime` verbatim. The four scan files affected
(`model/common.rs`, `options/loading/common.rs`,
`options/loading/traits.rs`, `options/loading/impls.rs`) swap their
imports from `crate::runner::manifest::*` to `effigy_manifest::*`
directly — zero behavior change.

`load_task_manifest` already lives in `effigy-manifest`; the runner
keeps only a thin `ManifestError → RunnerError` mapper at
`src/runner/manifest.rs:33-46`. Scan calls
`effigy_manifest::load_task_manifest` and maps `ManifestError` →
`ScanError` itself. No relocation of `load_task_manifest` is needed.

`TASK_MANIFEST_FILE` is a one-line constant (`"effigy.toml"`) used in
one scan file (`options/loading/common.rs`). Travels into the new
crate as a crate-private constant, matching the routing extraction's
handling of the same constant.

### 5. Prerequisite ordering vs card `248` — independent

Scan has zero overlap with five of card `248`'s six relocation targets
(`runner::util::*`, `runner::tooling::vitest_command_for_js_package_manager`,
`runner::render::encode_json`, `data_loading::*`, `testing::detect_test_runner_plans`,
`BUILTIN_TASKS`, `DEFAULT_BUILTIN_TEST_MAX_PARALLEL`). The only
touchpoint is `TASK_MANIFEST_FILE` — one scan file, and scan handles
it directly per decision 4 (inline into `effigy-scan`). `247`/`249`
and `248` can run in any order.

### 6. Test redistribution — 12 tests travel with the crate

All 12 `#[test]` functions sit in `src/runner/scan/tests.rs`. Other
scan files have `#[cfg(test)]` gates but no `#[test]` bodies. Tests
use only in-crate types + `serde_json` + `std::path` — clean port,
no fixture rewrites. Runner lib count drops by 12 at extraction; new
`effigy-scan` crate count picks up 12.

### 7. Scope shape — single implement card, no prerequisite

Diverges from the `245`+`246` cadence intentionally. Routing's
prerequisite card existed because catalog had **real glue
consolidation** work: catalog owned a manifest-loading path distinct
from scan's, and that split had to land inside the runner first. Scan
has **no equivalent work** — its manifest reach is already pure
re-exports (zero-behavior imports), its error surface is 20 uniform
`task_invocation` calls, and no other subsystem parallels scan's
manifest-loading use. The prerequisite would be purely cosmetic
import swaps plus the `ScanError` introduction, all of which can
land in the same commit as the crate move without reviewer strain.

Card [`249`](./249-implement-effigy-scan-extraction.md) stays as the
single implement card. No prereq card opens.

### Hidden coupling findings

- Scan reaches no runner types beyond `RunnerError::task_invocation`.
  No runner helpers, globals, or non-manifest types consumed.
- External deps the new crate needs: `globset`, `ignore`, `serde`,
  `serde_json` (tests only). All already in the workspace `Cargo.toml`.
  Zero `regex` / `rayon` / `walkdir` / `toml` direct use in scan.
- 328 `pub(in crate::runner)` visibility markers across 34 scan files.
  At the new crate boundary they become `pub` (for items the runner
  re-exports) or `pub(crate)` (crate-internal). Heavy count but
  mechanical.
- Inbound callers outside scan reach into `runner::scan::model::*`
  for four items (`ScanRenderFormat`, threshold types, severity
  types) — those become the crate's public surface.
- No cyclic risk. Scan depends on `effigy-manifest`;
  `effigy-manifest` has no reach into scan. `effigy-managed` names
  `"effigy-scan"` only as a string slug, not a cargo dep.
- No runner-specific test fixtures. All 12 tests use plain stdlib +
  `serde_json`.

## Next Task

Card [`249-implement-effigy-scan-extraction.md`](./249-implement-effigy-scan-extraction.md)
is now the active ready implement card for the scan sub-lane. It does
the full move in one go: introduce `ScanError`, relocate
`src/runner/scan/**` into `crates/effigy-scan/src/**`, swap manifest
re-exports for direct `effigy_manifest::*` imports at the four affected
files, flip visibilities, wire `Cargo.toml`, migrate the ~11
caller files (builtin/scan, doctor scan-checks, builtin/watch/runtime,
builtin/registry) to the new import path. No prerequisite card opens.

Card [`248`](./248-implement-runner-utility-prerequisites-for-effigy-builtin.md)
stays ready and independent — it can execute in parallel with `249`.
