# 247 Decide Effigy-Scan Extraction Shape

Status: ready
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

## Next Task

_To be written in the `Decision` section once sweep + review complete.
Expected outcome: open [`249-implement-effigy-scan-extraction.md`](./249-implement-effigy-scan-extraction.md)
as the implement card, possibly preceded by a small prerequisite card
if sweep finds a hazard not yet identified._
