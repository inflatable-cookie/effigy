# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards; archive stale cards
  once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of every historical card

## Current Live Chain

- [`252-implement-runner-root-shim-and-directory-tidy.md`](./252-implement-runner-root-shim-and-directory-tidy.md)
  is complete. Retired four single-call-site shims
  (`env_schema_support.rs`, `render.rs`, `model/constants.rs`,
  `cli/runner_dispatch.rs`), moved `deferred_builtins_*` and the
  deferral constants into `runner/deferral/`, and flattened three
  over-nested subtrees (`doctor/run/workflow` →
  `doctor/workflow/`, `doctor/run/check_registry` →
  `doctor/checks/`, `execute/selection/fallback/` → single file).
  `runner/manifest.rs` left in place after survey showed it's not
  a pure shim (four live helpers in active use).
  `super::super::super::super::` count 12 → 0;
  `super::super::super::` count 37 → 15.
- [`253-decide-effigy-doctor-runner-extraction-shape.md`](./253-decide-effigy-doctor-runner-extraction-shape.md)
  is complete. Decision: fold into the existing `effigy-doctor`
  library crate (not a new `effigy-doctor-runner`), growing it
  from pure-library to domain orchestration at ~5.2k LOC.
  `DoctorError` enum with five variants (`DoctorNonZero`,
  `TaskInvocation`, `Ui`, `Manifest`, `Scan`). No port traits —
  doctor depends directly on `effigy-manifest`, `effigy-scan`,
  `effigy-core`, `effigy-env`, `effigy-tasks`, `effigy-routing`,
  `effigy-ui`. Single crate, no cluster. No prerequisite cards —
  card `252` already handled the directory flattens.
- [`254-implement-effigy-doctor-runner-extraction.md`](./254-implement-effigy-doctor-runner-extraction.md)
  is the ready card for `g02.010`. Mechanical crate move per card
  `253`'s decision: grow `effigy-doctor/Cargo.toml` deps, move
  all 65 files, introduce `DoctorError`, add
  `From<DoctorError> for RunnerError` at runner boundary, inline
  the 2 `current_working_dir()` call sites, flip the
  `ManifestJsPackageManager` import to `effigy-manifest` direct.
- [`255-implement-test-harness-prelude-flatten.md`](./255-implement-test-harness-prelude-flatten.md)
  is queued — closes the lane by collapsing the nested test-side
  prelude chain into a single top-level fixture surface.
- [`250-implement-effigy-builtin-extraction.md`](./250-implement-effigy-builtin-extraction.md)
  is complete. `src/runner/builtin/**` (120 files, ~10,114 lines)
  moved into the new `effigy-builtin` workspace crate behind a
  narrow `BuiltinError` → `RunnerError` boundary. The
  `BuiltinRuntimePorts` trait definition plus `LockScope`,
  `UnlockResult`, `TaskCacheEntry` moved with it; the runner keeps
  the concrete `RunnerBuiltinPorts` impl. `BUILTIN_TASKS` and
  `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` now live in the new crate as
  `pub const`s.
- [`251-implement-builtin-runtime-ports-inversion.md`](./251-implement-builtin-runtime-ports-inversion.md)
  is complete. The port-trait inversion landed with 13 methods (not
  16 as proposed — three unused cache-lifecycle methods dropped after
  grep confirmed builtin doesn't consume them). Pure-helper
  relocations shipped: `parse_task_selector` → `effigy-tasks`,
  `with_local_node_bin_path` direct from `effigy_core::shell` at
  every site, `render::{plain_renderer, render_utf8, text_renderer,
  encode_json}` → `effigy-ui`.
- [`115-implement-effigy-distribution-release-closure.md`](./115-implement-effigy-distribution-release-closure.md)
  is complete. Release execution remains deferred until the `g02.010` thread
  closes cleanly.

## Archive Rule

- closed or paused lane cards should move to `../archive/batch-cards/` once the
  lane no longer needs them in the active tree
- the active tree should stay focused on the live strict lanes rather than the
  full historical corpus
- use the governing spec plus roadmap to resolve the current ready card; this
  README is only the front door

## Next Task

Execute card `254` — move `src/runner/doctor/**` into the existing
`effigy-doctor` crate per card `253`'s decision. Mechanical
extraction: 65 files, ~4.5k LOC, single commit, narrow
`DoctorError` → `RunnerError` boundary at the runner's edge.
