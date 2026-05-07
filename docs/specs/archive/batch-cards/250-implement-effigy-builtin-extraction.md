# 250 Implement Effigy-Builtin Extraction

Status: done
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract `src/runner/builtin/**` (~10,114 lines, 120 files) into a new
`effigy-builtin` workspace crate. Follow the established extraction
pattern (`effigy-process`, `effigy-ui`, `effigy-managed`, `effigy-routing`,
`effigy-scan`): single crate, narrow `BuiltinError` enum,
`impl From<BuiltinError> for RunnerError` at the runner's edge, call
sites migrate to the new crate's import path.

Card `244` decided the shape (single crate, direct migration, Job-8
error boundary). Cards `247`, `248`, `249` discharged the first
prerequisite wave. Card `251` — added after `244`'s coupling sweep
was found incomplete — inverts builtin's remaining runtime reach-ins
behind a single port trait and relocates the pure-helper surface
(`parse_task_selector`, `with_local_node_bin_path`, `render::*`)
into owner crates. With `251` landed, this card executes the full
crate move in one commit.

## Context

With scan extracted (card `249`), `effigy-scan` is a clean cross-crate
dep for `builtin/scan/**`'s orchestration layer. Utility prerequisites
(card `248`) relocated `parse_json`/`parse_toml`/`read_utf8` into
`effigy-core::data_loading`, `TASK_MANIFEST_FILE` into `effigy-manifest`,
`shell_quote` / `parse_dotenv_entries` / `normalize_builtin_test_suite`
to direct consumption from their owner crates, `detect_test_runner_plans`
into `effigy-tasks::testing`, and inverted `deferred_builtins_for_root`
out of `builtin/support.rs`. Port inversion (card `251`) collapses
the reach-in surface for `unlock`, `watch`, `cache`, `test`,
`tasks`, and `doctor` behind `BuiltinRuntimePorts`, and relocates
the render / util helpers so no builtin file imports a runner-
internal path.

Scope:

| Subsystem | Path | Files | Lines |
|---|---|---:|---:|
| Dispatcher / entry | `mod.rs`, `registry.rs`, `response.rs`, `support.rs`, `command_spec*`, `doc_render.rs`, `help_text*` | 8 | ~720 |
| `test/**` | `builtin/test.rs` + subtree | 24 | ~2,385 |
| `scan/**` | `builtin/scan.rs` + subtree | 22 | ~1,982 |
| `config/**` | `builtin/config.rs` + subtree | 12 | ~1,606 |
| `completion/**` | `builtin/completion.rs` + subtree | 17 | ~1,027 |
| `arg_parser/**` | `builtin/arg_parser.rs` + subtree | 6 | ~749 |
| `watch/**` | `builtin/watch.rs` + subtree | 7 | ~514 |
| `cache/**` | `builtin/cache.rs` + subtree | 6 | ~462 |
| `migrate/**` | `builtin/migrate.rs` + subtree | 7 | ~452 |
| `unlock/**` | `builtin/unlock.rs` + subtree | 5 | ~292 |
| `init/**` | `builtin/init.rs` + subtree | 5 | ~248 |
| `help/**`, `help_text/**` | 4 | ~134 |
| `doctor/**` | `builtin/doctor.rs` + subtree | 3 | ~113 |
| `tasks/**` | `builtin/tasks.rs` + subtree | 3 | ~133 |
| `registry/**` | `builtin/registry/**` subtree | 2 | ~186 |
| `text_doc/**` | `builtin/text_doc.rs` + subtree | 3 | ~84 |
| `test_support.rs` | 1 | ~19 |
| **Total** | | **120** | **~10,114** |

### Residual runner reach-ins (resolve at extraction)

After cards `248` and `251`, builtin still reaches for these
runner-side surfaces:

- `crate::runner::manifest::{LoadedTaskManifest, ...}` —
  `builtin/config/output.rs` (imports plus two fully-qualified
  references at lines 281 and 412). Swap to `effigy_manifest::*`
  directly — `runner::manifest.rs` lines 7–14 are pure re-exports
  (verified during card `249`). Same applies to
  `builtin/test/execution.rs` imports of `ManifestCargoEnvMatchMode`
  and `ManifestTestSuiteTeardownPolicy`, and
  `builtin/test/render/results/payload.rs` / `test/planning/model.rs`
  consumers of the same types.
- `crate::runner::model::constants::{BUILTIN_TASKS, DEFAULT_BUILTIN_TEST_MAX_PARALLEL}`
  — two builtin consumers (`completion/scripts/command_index.rs`,
  `test/planning/config.rs`). Card `248` deferred these with a plan
  to inline into `effigy-builtin` at extraction. Inline the constants
  inside the new crate. Two non-builtin callers remain
  (`tasks_probe/resolve.rs`, `tasks_listing/row_projection.rs`); expose
  `BUILTIN_TASKS` from the new crate via `pub const` so those switch
  to `effigy_builtin::BUILTIN_TASKS`.
- `crate::runner::builtin::test_support` is re-exported through
  `src/runner/test_support.rs` for fixtures. Flip the runner-side
  `test_support.rs` re-exports to pull from `effigy_builtin::test_support::*`.
- `BuiltinRuntimePorts` trait (introduced by card `251` at
  `src/runner/builtin_ports.rs`) — trait definition moves into
  `effigy-builtin` as a `pub trait`; the runner keeps the concrete
  `RunnerBuiltinPorts` implementation, importing the trait from
  `effigy_builtin::BuiltinRuntimePorts`. Associated types re-exported
  from the port module (`LockScope`, `UnlockResult`, `TaskCacheEntry`,
  `TaskCacheCheck`) move with the trait; the runner imports them
  back from `effigy_builtin::*` for the concrete impl.

Card `251` eliminates every other reach-in the original sweep missed
(`runner::locking::*`, `runner::cache::{ops,model}::*`,
`runner::execute::run_manifest_task_with_cwd`,
`runner::command_context::current_working_dir`,
`runner::tasks_command::run_tasks`, `runner::doctor::run_doctor`,
`runner::deferred_builtins_from_catalogs`, `runner::util::{
parse_task_selector, with_local_node_bin_path}`,
`runner::render::{plain_renderer, render_utf8, text_renderer,
encode_json}`). Card `250` does not re-resolve them.

### Inbound callers to migrate

- `src/runner/execute/selection/fallback/builtin.rs` —
  `try_run_builtin_task` dispatch site.
- `src/runner/mod.rs` — registry lookup / deferred-builtins integration
  (BUILTIN_TASKS, `deferred_builtins_from_catalogs` call sites).
- `src/runner/test_support.rs` — fixture re-exports from
  `builtin::test_support`.
- `src/runner/tasks_probe/resolve.rs`,
  `src/runner/tasks_listing/row_projection.rs` — `BUILTIN_TASKS`
  consumers switch to `effigy_builtin::BUILTIN_TASKS`.

CLI-side reaches (`src/cli/entrypoint.rs`,
`src/cli/help_dispatch.rs`) call
`crate::runner::deferred_builtins_for_root`, which is a runner-owned
helper — unchanged by this card.

## In Scope

- Create `crates/effigy-builtin/` workspace crate with `Cargo.toml`,
  `src/lib.rs`.
- Move `src/runner/builtin/**` contents into
  `crates/effigy-builtin/src/`, including `test_support.rs`.
- Introduce `BuiltinError` enum inside the new crate. Shape:
  - `Invocation(String)` covering the ~54 `task_invocation(...)` /
    `task_invocation_failed_{read,parse,write,render}` call sites.
  - `Manifest(ManifestError)` bridging builtin/config and
    builtin/test paths that touch `effigy_manifest::load_task_manifest`.
  - `BuiltinTestNonZero { failures, rendered }` — one call site in
    `test/render/results.rs`.
  - `BuiltinScanNonZero { finding_count, rendered }` — one call site
    in `scan/execution/core/response.rs`.
  - `Ui(String)` — four call sites across
    `completion/candidates/cache/mod.rs` and `test/execution.rs`.
  - `CommandLaunch { command, error }` — carries the two
    `RunnerError::TaskCommandLaunch` constructions in
    `test/execution.rs`.
  - `TaskLockIo { path, error }` / `TaskLockConflict { ... }` —
    propagate lock-side failures surfaced through
    `BuiltinRuntimePorts::acquire_scopes` / `unlock_*`. Produced
    only by the port trait's default `From<RunnerError>` bridge at
    the call sites; the runner's `From<BuiltinError> for RunnerError`
    mirrors them back. Alternative: keep the port trait returning
    `RunnerError` at this boundary — decide during execution
    whichever is cleaner.
- Rewrite every producer inside `builtin/**` to return `BuiltinError`;
  adapters at call sites lift via `?`.
- Add `impl From<BuiltinError> for RunnerError` in
  `src/runner/error.rs` that lifts each `BuiltinError` variant to the
  matching `RunnerError::*` shape (reusing `map_manifest_error` for
  the `Manifest` variant, matching the `From<ScanError>` pattern).
- Move the `BuiltinRuntimePorts` trait definition from
  `src/runner/builtin_ports.rs` into `crates/effigy-builtin/src/ports.rs`
  as a `pub trait`. The runner keeps the `RunnerBuiltinPorts`
  concrete impl, now importing the trait from
  `effigy_builtin::BuiltinRuntimePorts`. The associated types
  surfaced by the trait (`LockScope`, `UnlockResult`,
  `TaskCacheEntry`, `TaskCacheCheck`) move with the trait and become
  `pub` in the new crate; the runner's `locking` / `cache` modules
  import them back from `effigy_builtin::*` for the concrete impl.
- Swap builtin files that import `crate::runner::manifest::*` to
  `effigy_manifest::*` directly (verified pure re-exports).
- Inline `BUILTIN_TASKS` and `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` into
  the new crate as `pub const`s. Remove from
  `src/runner/model/constants.rs`. Migrate the two non-builtin
  callers (`tasks_probe/resolve.rs`, `tasks_listing/row_projection.rs`)
  to `effigy_builtin::BUILTIN_TASKS`.
- Flip `pub(in crate::runner::builtin::...)` / `pub(in crate::runner)`
  visibility markers inside the moved tree to `pub` or `pub(crate)`
  as required by the new crate boundary.
- Add crate deps to `effigy-builtin/Cargo.toml`: `effigy-core`,
  `effigy-manifest`, `effigy-tasks`, `effigy-routing`, `effigy-scan`,
  `effigy-managed`, `effigy-ui`, `effigy-process`, `effigy-env`,
  `serde`, `serde_json`, `toml` (as required by moved callers;
  prune on first compile).
- Remove `src/runner/builtin/` directory from the runner.
- Migrate caller files to import from `effigy_builtin::*`:
  - `src/runner/execute/selection/fallback/builtin.rs`
  - `src/runner/mod.rs`
  - `src/runner/test_support.rs`
  - `src/runner/tasks_probe/resolve.rs`
  - `src/runner/tasks_listing/row_projection.rs`
  - `src/runner/builtin_ports/runner_impl.rs` (trait import now from
    `effigy_builtin`; types re-imported for the concrete impl).
- Update `Cargo.toml` workspace `members` list.
- Add `effigy-builtin = { path = "crates/effigy-builtin" }` to the
  root crate's deps.

## Out Of Scope

- Any cluster split (`effigy-builtin-test`, `effigy-builtin-scan`,
  etc.) — card `244` ruled this out.
- Extraction or reshaping of `runner::locking`, `runner::deferral`,
  `runner::command_context`, `runner::cache`, `runner::execute`
  (none of these are builtin-side).
- Changes to `RunnerError`'s variant shapes beyond adding the
  `From<BuiltinError>` impl.
- Relocating runner-owned `deferred_builtins_for_root` (used by CLI
  help dispatch; unchanged by this card).
- Moving non-builtin `runner::render::encode_json` consumers.
- Any post-extraction follow-up refactors (richer `BuiltinError`
  variants, cluster splits, crate-internal reorganization) — all
  deferred to post-extraction checkpoints.

## Acceptance Criteria

- `effigy-builtin` workspace crate exists with the moved code.
- `src/runner/builtin/` directory is gone.
- `impl From<BuiltinError> for RunnerError` lives in
  `src/runner/error.rs` and depends on `effigy_builtin::BuiltinError`.
- All caller files import from `effigy_builtin::` directly (no
  transitional shim inside `src/` — per `242` / second-sweep lesson).
- `src/lib.rs` does not gain a `pub use effigy_builtin::*` re-export —
  consumers import from the crate directly.
- `BUILTIN_TASKS` and `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` live inside
  `effigy-builtin` as `pub const`s; `runner/model/constants.rs` sheds
  both.
- `BuiltinRuntimePorts` trait lives in `effigy-builtin`; runner
  imports it via `effigy_builtin::BuiltinRuntimePorts` for the
  `RunnerBuiltinPorts` impl.
- No `builtin/**` file imports `crate::runner::manifest::*`,
  `crate::runner::model::constants::*`, or any
  `super::super::super::*` path reaching into the runner tree.
- `src/runner/test_support.rs` re-exports from
  `effigy_builtin::test_support::*` rather than `builtin::test_support`.
- Test totals flag exact deltas in the post-extraction checkpoint:
  runner lib drops by the builtin-side `#[test]` count; new
  `effigy-builtin` test count picks up the same.

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Close the `g02.010` strict lane after this card lands. Spec 010's
remaining bounded batch was "built-in tasks" — once extracted, the
lane moves to a pause-boundary decide card that sets up the next
roadmap pivot (likely release-closure resumption per card `115`'s
deferred status, or a follow-up modularization pause decision).
