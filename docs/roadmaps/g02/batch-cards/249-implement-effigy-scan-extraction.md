# 249 Implement Effigy-Scan Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract `src/runner/scan/**` (~4,928 lines) into a new `effigy-scan`
workspace crate. Follow the established extraction pattern
(`effigy-process`, `effigy-ui`, `effigy-managed`, `effigy-routing`):
new crate with narrow `ScanError`, `From<ScanError> for RunnerError` at
the runner's edge, call sites migrate to the new crate's import path.

Card `247` decided the shape. No prerequisite card — the whole move
lands in one go. Independent of card `248`; either can run first.

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

- Create `crates/effigy-scan/` workspace crate with `Cargo.toml`,
  `src/lib.rs`.
- Move `src/runner/scan/**` contents into `crates/effigy-scan/src/`,
  including `tests.rs` and `constants.rs`.
- Introduce `ScanError` enum inside the new crate. Initial shape is a
  single-variant newtype (`ScanError::Invocation(String)`) covering
  the 20 `RunnerError::task_invocation(...)` call sites; enriching to
  a richer enum (`InvalidGlob`, `ReadFailed`, `ValidateBounds`, etc.)
  is a follow-up inside the crate. Rewrite every producer inside
  `scan/**` to return `ScanError`; adapters at call sites lift via
  `?`.
- Add `impl From<ScanError> for RunnerError` in `src/runner/error.rs`
  that lifts every `ScanError` variant to
  `RunnerError::task_invocation(...)`.
- Swap the four scan files that import `crate::runner::manifest::*`
  (`model/common.rs`, `options/loading/common.rs`,
  `options/loading/traits.rs`, `options/loading/impls.rs`) to import
  from `effigy_manifest::*` directly. `src/runner/manifest.rs` lines
  7-14 are pure re-exports of the same items, so the swap is
  behavior-preserving.
- Pull `TASK_MANIFEST_FILE` into the new crate as a crate-private
  constant (the one scan consumer, `options/loading/common.rs`,
  imports it locally). Runner's own copy in
  `src/runner/model/constants.rs` stays put for non-scan consumers.
- Flip `pub(in crate::runner)` visibility markers in scan files to
  `pub` or `pub(crate)` as required by the new crate boundary.
- Add crate deps: `effigy-manifest`, `effigy-tasks`, `globset`,
  `ignore`, `serde`, `serde_json` (tests). No `effigy-core` dep
  required (scan reaches no `effigy-core` types today). No
  `effigy-ui` dep required (render uses its own stringify helpers,
  not widget types).
- Remove `src/runner/scan/` directory from the runner.
- Migrate caller files to import from `effigy_scan::*`:
  - `src/runner/builtin/scan/**` (all files touching `scan::*`)
  - `src/runner/builtin/watch/runtime.rs`
  - `src/runner/builtin/registry.rs`
  - `src/runner/doctor/{attention_markers,comment_ratio,duplicate_blocks,generated_assets,generated_in_src,god_files,manifest,scan_checks/mod,stale_suppressions}.rs`
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
- `impl From<ScanError> for RunnerError` lives in `src/runner/error.rs`
  and depends on `effigy_scan::ScanError`.
- All caller files import from `effigy_scan::` directly (no transitional
  shim inside `src/` — per `242` / second-sweep lesson).
- `src/lib.rs` does not gain a `pub use effigy_scan::*` re-export —
  consumers import from the crate directly.
- `builtin/scan/**` imports flip to `effigy_scan::*` as part of the
  caller migration.
- Doctor's scan-check modules continue to emit identical findings —
  validate via existing doctor tests.
- Test totals: runner lib drops by ~12 (scan's `#[test]` functions);
  new `effigy-scan` unit-test count picks up those 12; `effigy-managed`
  (16) and `effigy-env` (89) unchanged. Flag exact deltas in the
  post-extraction checkpoint.

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

Landed 2026-04-17. `crates/effigy-scan/` created with the full move
in a single commit; ~4,928 lines across 34 files relocated. `ScanError`
shipped as a two-variant enum (`Invocation(String)`,
`Manifest(ManifestError)`); the `Manifest` variant bridges scan's
option-loading path through `effigy_manifest::load_task_manifest`
(no runner-side wrapper needed, `ScanError` implements
`From<ManifestError>`). `From<ScanError> for RunnerError` in
`src/runner/error.rs` lifts `Invocation` to `TaskInvocation` and
delegates `Manifest` through the existing `map_manifest_error`
helper. 328 `pub(in crate::runner)` markers flipped to `pub` inside
the crate for items reachable via the crate-root re-exports; the
crate root (`lib.rs`) re-exports a flattened public surface so callers
use `effigy_scan::ScanRenderFormat`, `effigy_scan::run_god_file_scan_workspace`,
etc. (no submodule paths externally). 19 caller files migrated —
`builtin/scan/**` and `doctor/**` — with `.map_err(Into::into)`
closure wrapping at the 21 sites where `ScanError` meets a
runner-side `RunnerError` bound (14 in `builtin/scan/execution/modes.rs`,
7 in doctor check files). `TASK_MANIFEST_FILE` inlined as a
crate-private constant. Test totals: runner lib −12, `effigy-scan`
+12 (all 12 `#[test]` fns in the former `scan/tests.rs`).
`effigy-managed` (16) and `effigy-env` (89) unchanged.

Full validation green: `cargo build`, `cargo fmt --check`, `cargo
clippy` (-D warnings, standard allowlist), `cargo test --workspace`,
`cargo run --bin effigy -- qa:docs`.

## Next Task

Draft the `effigy-builtin` implement card (tentatively `250`+).
All three prerequisites (`247` decide scan, `248` utility prereqs,
`249` implement scan) are now complete. Card `244` already fixed
the scope, error boundary, and migration pattern.
