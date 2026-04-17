# 249 Implement Effigy-Scan Extraction

Status: queued
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract `src/runner/scan/**` (~4,928 lines) into a new `effigy-scan`
workspace crate. Follow the established extraction pattern
(`effigy-process`, `effigy-ui`, `effigy-managed`, `effigy-routing`): new
crate, narrow error boundary per the `247` decision, `From` impl at the
runner's edge (if applicable), call sites migrate to the new crate's
import path.

This card is queued behind card `247` (decide scan shape). It also
benefits from card `248` landing first (runner-utility prereqs) to
reduce the migration radius. Fields below are the provisional scope;
card `247`'s Decision section may trim or shift them.

## Context

Card `244` decided the `effigy-builtin` scope and established that
`builtin/scan/**` is orchestration over this engine, not a duplicate.
Extracting scan as its own crate is a prerequisite for the future
`effigy-builtin` implement card (not yet drafted).

Scope (subject to the `247` decision):

| Subsystem | Path | Lines |
|---|---|---|
| Model types | `scan/model/**` | ~541 |
| Support helpers | `scan/support/**`, `scan/support.rs` | ~1,150 |
| Execution | `scan/execution/**` | ~1,080 |
| Options / loading | `scan/options/**`, `scan/options.rs` | ~700 |
| Render | `scan/render/**` | ~1,260 |
| Tests | `scan/tests.rs` | 197 |
| Constants | `scan/constants.rs` | ~15 |
| **Total** | | **~4,928** |

Inbound callers to migrate (from the `244` sweep; ~11 files plus
`doctor/**` adapters):

- `src/runner/builtin/scan/**` — all files that touch
  `crate::runner::scan::*`.
- `src/runner/builtin/watch/runtime.rs`.
- `src/runner/builtin/registry.rs`.
- `src/runner/doctor/{attention_markers,comment_ratio,duplicate_blocks,
  generated_assets,generated_in_src,god_files,manifest,scan_checks,
  stale_suppressions}.rs`.

## In Scope

_Final shape to be locked by card `247`. Provisional scope below:_

- Create `crates/effigy-scan/` workspace crate with `Cargo.toml`,
  `src/lib.rs`.
- Move `src/runner/scan/**` contents into `crates/effigy-scan/src/`.
- Per `247` decision:
  - Introduce `ScanError` with `From<ScanError> for RunnerError`, or
    keep scan surfacing `RunnerError::task_invocation(...)` via a
    constructor re-export.
  - Resolve the manifest-loading glue (consolidated scan-owned copy,
    relocated into `effigy-manifest`, or keep options-loading behind
    in the runner — locked by `247` §4).
- Add deps: `effigy-core`, `effigy-manifest`, `effigy-tasks`,
  `effigy-ui` (for render), `std` / `serde` / `toml` as required.
- Remove `src/runner/scan/` directory from the runner.
- Update `src/runner/error.rs` if a `ScanError` is introduced
  (`impl From<ScanError> for RunnerError`).
- Migrate every caller file listed above to import from `effigy_scan::*`.
- Update `Cargo.toml` workspace `members` list.
- Add `effigy-scan = { path = "crates/effigy-scan" }` to the root
  crate's deps.

## Out Of Scope

- Any `builtin/scan/**` content moves (that is the future
  `effigy-builtin` implement card).
- Changes to `RunnerError`'s variant shapes.
- `runner/manifest.rs` non-scan consumers.
- Relocating utilities — those belong to card `248`.

## Acceptance Criteria

- `effigy-scan` workspace crate exists with the moved code.
- `src/runner/scan/` directory is gone.
- If `247` selects a `ScanError` boundary:
  `impl From<ScanError> for RunnerError` lives in `src/runner/error.rs`
  and depends on `effigy_scan::ScanError`.
- All caller files import from `effigy_scan::` directly (no transitional
  shim inside `src/` — per `242` / second-sweep lesson).
- `src/lib.rs` does not gain a `pub use effigy_scan::*` re-export —
  consumers import from the crate directly.
- `builtin/scan/**` continues to compile (still pointing at
  `crate::runner::scan::*` before this card moves them? — see note).
  After this card lands, builtin/scan's imports flip to `effigy_scan::*`
  as part of the caller migration.
- Doctor's scan-check modules continue to emit identical findings —
  validate via existing doctor tests.
- Test totals unchanged: 683 runner lib + 16 effigy-managed + 89
  effigy-env + any new effigy-scan unit tests (flag in post-extraction
  checkpoint).

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

_To be decided after this card lands. Expected: draft the
`effigy-builtin` implement card (tentatively `250`+) once cards `247`,
`248`, and `249` are all complete._
