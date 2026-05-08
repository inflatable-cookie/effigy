# 254 Implement Effigy-Doctor Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move `src/runner/doctor/**` (~4,532 lines, 65 files) into the
existing `effigy-doctor` workspace crate per card `253`'s decision.
Grow `effigy-doctor` from a pure library (reports, findings,
projections, contracts — 705 LOC) into the domain crate that owns
both the types and the doctor orchestration. Narrow `DoctorError`
→ `RunnerError` boundary at the runner's edge.

## Context

Card `253` decided:

- **Crate:** fold into existing `effigy-doctor` (no new crate name).
- **Error:** `DoctorError` enum in `effigy-doctor` with variants
  `DoctorNonZero`, `TaskInvocation`, `Ui`, `Manifest(ManifestError)`,
  `Scan(ScanError)`. `impl From<DoctorError> for RunnerError` at
  runner boundary.
- **Port surface:** none. Doctor depends directly on
  `effigy-manifest`, `effigy-scan`, `effigy-core`, `effigy-env`,
  `effigy-tasks`, `effigy-routing`, `effigy-ui`.
- **Split shape:** single crate.
- **Prerequisites:** none (card `252` already handled the directory
  flattens and shim inlines).

Post-card-252 doctor reach-ins into the runner are minimal:
- `RunnerError` (bridged via `From<DoctorError> for RunnerError`)
- `command_context::current_working_dir()` — 2 call sites, ~5 LOC of
  `std::env::current_dir()` + error mapping
- `ManifestJsPackageManager` — 1 call site, type already lives in
  `effigy-manifest`

## In Scope

- Extend `crates/effigy-doctor/Cargo.toml` with the domain deps
  that the moved orchestration needs: `effigy-scan`, `effigy-core`,
  `effigy-env`, `effigy-tasks`, `effigy-routing`, `effigy-ui`,
  `effigy-cli` (for `DoctorArgs`), plus any transitive helpers
  (`serde`, `serde_json`, `toml`, `walkdir`, etc. as required).
- Move every file under `src/runner/doctor/**` into
  `crates/effigy-doctor/src/`. Preserve the post-card-252
  directory shape (`workflow/`, `checks/`, `render/`, `explain/`,
  `manifest/`, `scan_checks/`, `finding_templates/`,
  `health/`, `report/`, `task_graph/`, `contracts/`, plus root
  modules).
- Introduce `DoctorError` in `crates/effigy-doctor/src/error.rs`
  with the five variants from the decision. Helper constructors
  mirror the `BuiltinError` pattern:
  - `task_invocation(message)`
  - `task_invocation_failed_read(path, err)`
  - `task_invocation_failed_parse(path, err)`
  - `task_invocation_failed_write(path, err)`
  - `task_invocation_failed_render(path, err)`
  - `From<ManifestError>`, `From<ScanError>`, `From<UiError>`.
- Rewrite every producer inside the moved tree to return
  `DoctorError` instead of `RunnerError`. `?` adapters at call
  sites use the new `From` impls.
- Add `impl From<DoctorError> for RunnerError` in
  `src/runner/error.rs`:
  - `DoctorNonZero { error_count, rendered }` → `Self::DoctorNonZero { .. }`
  - `TaskInvocation(m)` → `Self::TaskInvocation(m)`
  - `Ui(m)` → `Self::Ui(m)`
  - `Manifest(e)` → `map_manifest_error(e)` (reuse existing helper)
  - `Scan(e)` → `Self::from(e)` (reuse existing `From<ScanError>`)
- Runner side: collapse `src/runner/doctor.rs` into a thin shim
  or drop it entirely. Preferred: drop it and have
  `src/runner/entrypoints/dispatch.rs` call `effigy_doctor::run_doctor`
  directly with a `.map_err(RunnerError::from)` lift. Alternative
  (if call-site count is large enough to justify): keep
  `src/runner/doctor.rs` as a one-line shim.
- Inline `current_working_dir()` at the 2 call sites: replace with
  `std::env::current_dir().map_err(|error| DoctorError::task_invocation(...))`.
  If `effigy-core` already exposes this helper, import from there
  instead.
- Flip `report/snapshot.rs` import from
  `crate::runner::manifest::config_sections::ManifestJsPackageManager`
  to `effigy_manifest::config_sections::ManifestJsPackageManager`
  directly.
- Update the workspace `Cargo.toml` if any new inter-crate deps
  need to be declared (most already exist in the main root
  crate's deps list).
- Migrate any inline `#[test]` modules that reach into
  runner-internal types. Integration tests under `src/tests/` stay
  where they are and continue to exercise the public surface.
- Update the entry-point dispatch:
  `src/runner/entrypoints/dispatch.rs` or whichever file currently
  matches on `Command::Doctor` replaces the call from the runner's
  doctor module to `effigy_doctor::run_doctor`.

## Out Of Scope

- Any changes to the existing `effigy-doctor` types
  (`DoctorReport`, `DoctorFinding`, `DoctorSeverity`, etc.). They
  stay as-is; the extraction grows the crate around them.
- Port traits. Card `253` ruled them out.
- Reorganization of the moved tree beyond what card `252`
  already did. Internal cleanup (further flattening, renaming)
  stays for post-extraction follow-up if needed.
- Changes to `RunnerError` shape beyond adding the
  `From<DoctorError>` impl.
- Extraction of `runner::command_context` or `runner::manifest.rs`
  — they stay in the runner.
- CLI-side changes beyond the single `Command::Doctor` dispatch
  site.

## Acceptance Criteria

- `crates/effigy-doctor/src/` contains the moved doctor
  subsystem alongside the pre-existing report/finding/projection
  types.
- `src/runner/doctor/` directory gone.
- `DoctorError` exists in `effigy-doctor` with the five decided
  variants.
- `impl From<DoctorError> for RunnerError` lives in
  `src/runner/error.rs`.
- No file under `crates/effigy-doctor/src/` imports from
  `crate::runner::*` or `super::super::super::*` paths that reach
  into the runner tree.
- `src/runner/doctor.rs` either gone or a single-line shim.
- `cargo build --all-targets`, `cargo test --workspace`,
  `cargo fmt --all -- --check`, and
  `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity`
  all green.
- Runner lib test count drops by the inline doctor `#[test]`
  total; `effigy-doctor` test count picks up the same.

## Validation

- `cargo build --all-targets`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Next Task

Card `255` — flatten the test-harness prelude chain. That's the
final bounded cleanup in the reopened `g02.010` lane; once landed,
the lane closes and the roadmap returns to the release-closure
pivot (card `115`).
