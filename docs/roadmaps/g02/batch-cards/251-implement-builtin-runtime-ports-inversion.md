# 251 Implement Builtin Runtime Ports Inversion

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Invert the runtime reach-ins that `src/runner/builtin/**` makes into
the runner's inner machinery (`locking`, `cache::{ops,model}`,
`execute`, `command_context`, `tasks_command`, `doctor`, deferred-
builtins helper) behind a single `BuiltinRuntimePorts` trait the
runner implements and threads through the dispatcher. Relocate a
small set of pure helpers that are reach-ins in name only (`util::{
parse_task_selector, with_local_node_bin_path}` and `render::{
plain_renderer, render_utf8, text_renderer, encode_json}`) to their
owner crates so `builtin/**` no longer imports from runner-internal
paths for them.

After this card lands, `builtin/**` reaches the runner through
exactly one surface — the port trait — which collapses card `250`'s
crate move back to a mechanical extraction matching the
`effigy-scan` precedent.

## Context

Card `244`'s coupling sweep asserted that builtin did not reach into
`runner::locking`, `runner::deferral`, `runner::command_context`,
`runner::cache`, or `runner::execute`. That was incomplete. The
real reach-in surface surfaced while drafting card `250` execution:

| Reach-in target | Source site | Consumer builtin |
|---|---|---|
| `runner::locking::io::{acquire_scopes, unlock_all, unlock_scopes}` | `unlock.rs`, `watch/runtime.rs` | unlock, watch |
| `runner::locking::model::LockScope` | `unlock/request.rs`, `watch/runtime.rs` | unlock, watch |
| `runner::execute::run_manifest_task_with_cwd` | `watch/runtime.rs` | watch |
| `runner::cache::ops::{check_task_cache, update_task_cache_entry, cache_entries, cache_entry, invalidate_cache_keys, invalidate_all_cache_entries, cache_entry_key, task_cache_config}` | `cache/dispatch.rs` | cache |
| `runner::cache::model::TaskCacheEntry` | `cache/output.rs` | cache |
| `runner::command_context::current_working_dir` | `test/execution.rs` | test |
| `runner::tasks_command::run_tasks` | `tasks.rs` | tasks |
| `runner::doctor::run_doctor` | `doctor.rs` | doctor |
| `runner::deferred_builtins_from_catalogs` | `registry.rs` | help arm |
| `runner::util::{parse_task_selector, with_local_node_bin_path}` | `cache/selection.rs`, `test/execution.rs` | cache, test |
| `runner::render::{plain_renderer, render_utf8, text_renderer, encode_json}` | `config/reference.rs`, `response.rs`, `test/render/**`, `scan/execution/core/mod.rs` | config, response, test-render, scan orchestration |

The runner modules behind the first nine rows are the task runner's
core machinery (lock acquisition, cache store, execution pipeline,
top-level commands). Moving them out is a separate lane. Inverting
via a port trait is the minimum-surface move that unblocks builtin's
extraction.

Rows ten and eleven are pure helpers — no runtime state, no I/O
lifecycle. They belong in their owner crates, not the port trait.

## In Scope

### Port trait introduction

- Add `src/runner/builtin_ports.rs` defining:
  ```rust
  pub(in crate::runner) trait BuiltinRuntimePorts {
      fn acquire_scopes(&self, workspace_root: &Path, scopes: &[LockScope])
          -> Result<Vec<LockGuard>, RunnerError>;
      fn unlock_scopes(&self, workspace_root: &Path, scopes: &[LockScope])
          -> Result<UnlockResult, RunnerError>;
      fn unlock_all(&self, workspace_root: &Path)
          -> Result<UnlockResult, RunnerError>;
      fn run_manifest_task_with_cwd(&self, task: &TaskInvocation, cwd: PathBuf)
          -> Result<String, RunnerError>;
      fn current_working_dir(&self) -> Result<PathBuf, RunnerError>;
      fn run_tasks(&self, args: TasksArgs) -> Result<String, RunnerError>;
      fn run_doctor(&self, task: &TaskInvocation, runtime_args: &TaskRuntimeArgs,
          target_root: &Path) -> Result<Option<String>, RunnerError>;
      fn deferred_builtins_from_catalogs(&self, catalogs: &[LoadedCatalog],
          target_root: &Path) -> BTreeSet<String>;
      fn check_task_cache(&self, workspace_root: &Path, catalog_root: &Path,
          manifest_path: &Path, task_name: &str, task: &ManifestTask,
          command: &str) -> Result<TaskCacheCheck, RunnerError>;
      fn update_task_cache_entry(&self, workspace_root: &Path,
          catalog_root: &Path, manifest_path: &Path, task_name: &str,
          task: &ManifestTask, command: &str) -> Result<(), RunnerError>;
      fn cache_entries(&self, workspace_root: &Path)
          -> Result<Vec<TaskCacheEntry>, RunnerError>;
      fn cache_entry(&self, workspace_root: &Path, manifest_path: &Path,
          task_name: &str) -> Result<Option<TaskCacheEntry>, RunnerError>;
      fn invalidate_cache_keys(&self, workspace_root: &Path,
          keys: &[String]) -> Result<Vec<String>, RunnerError>;
      fn invalidate_all_cache_entries(&self, workspace_root: &Path)
          -> Result<usize, RunnerError>;
      fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String;
      fn task_cache_config<'a>(&self, task: &'a ManifestTask)
          -> Option<&'a ManifestTaskCache>;
  }
  ```
  Exact signatures pinned from the existing `pub(in crate::runner)`
  functions (see Context table source sites). Re-export the types the
  trait surfaces (`LockScope`, `LockGuard`, `UnlockResult`,
  `TaskCacheEntry`, `TaskCacheCheck`) from `builtin_ports` so builtin
  imports come through a single path.
- Add `src/runner/builtin_ports/runner_impl.rs` providing a
  `RunnerBuiltinPorts` zero-sized struct implementing the trait by
  forwarding to today's `pub(in crate::runner)` functions. No
  behavior change.

### Dispatcher threading

- `try_run_builtin_task` accepts `ports: &dyn BuiltinRuntimePorts` and
  passes it into `BuiltinDispatch::run`.
- `BuiltinDispatch::run` threads the port reference into each arm's
  entry function. Each entry function (`run_builtin_unlock`,
  `run_builtin_watch`, `run_builtin_cache`, `run_builtin_test`,
  `run_builtin_tasks`, `run_builtin_doctor`, `run_builtin_help`,
  `run_builtin_config`, `run_builtin_scan`, etc.) grows a `ports`
  parameter where it reaches into the runner's inner machinery.
  Arms that don't reach (`init`, `migrate`) skip the parameter.
- Caller `src/runner/execute/selection/fallback/builtin.rs`
  constructs `RunnerBuiltinPorts` and passes it to
  `try_run_builtin_task`.

### Builtin reach-in rewrites

- Every `super::super::super::{locking, execute, cache, command_context,
  tasks_command, doctor}::*` import in `builtin/**` deleted;
  call sites route through `ports.*` methods.
- `crate::runner::deferred_builtins_from_catalogs` call in
  `builtin/registry.rs` moved behind `ports.deferred_builtins_from_catalogs`.
- `crate::runner::builtin_ports::{LockScope, UnlockResult, ...}` used
  for the type names where builtin today reaches for
  `super::super::super::locking::model::LockScope` /
  `runner::cache::model::TaskCacheEntry`.

### Pure-helper relocations (248 pattern)

- `parse_task_selector` → `effigy-tasks` as `pub fn parse_task_selector`.
  The runner-side thin wrapper in `src/runner/util.rs` already
  delegates to `effigy_tasks::parse_task_selector` internally; the
  existing wrapper returns `RunnerError`, the new direct call returns
  `TaskError` which builtin call sites map via `RunnerError::from`
  (existing `From<TaskError> for RunnerError` impl). Two builtin
  callers flip to `effigy_tasks::parse_task_selector`. Three
  non-builtin callers (`demo_command.rs`, `tasks_listing/selection.rs`,
  test prelude) also flip. The runner-side wrapper stays only if
  some caller depends on the `RunnerError` signature; otherwise
  delete.
- `with_local_node_bin_path` → `effigy-core::shell::with_local_node_bin_path`.
  Card `248` already exposed it at `effigy_core::shell`; runner still
  keeps a thin shim because three non-builtin callers use it. Flip
  all callers to `effigy_core::shell::with_local_node_bin_path`
  directly and delete the `src/runner/util/shell.rs` shim's
  re-export.
- `render::{plain_renderer, render_utf8, text_renderer, encode_json}`
  → `effigy-ui` as `pub fn`s. `plain_renderer` / `render_utf8` /
  `text_renderer` are four- to twelve-line wrappers around
  `effigy-ui::Renderer`; relocating them is near-free. `encode_json`
  is a 20-line `serde_json::to_string_pretty` wrapper that
  `RunnerError::Ui`-lifts parse errors; rewrite it to return
  `UiError` (or a local error shape) and rely on `From<UiError> for
  RunnerError` at call sites. Eight runner-side callers plus the
  builtin sites flip to `effigy_ui::*`. Delete the runner-side
  `src/runner/render/{plain_renderer.rs, utf8.rs, text_renderer.rs,
  encode_json.rs}` (or their equivalent location inside
  `src/runner/render`).

## Out Of Scope

- Moving any `builtin/**` file — this card is inversion + relocation
  only.
- Creating `effigy-builtin` crate — that is card `250`.
- Introducing `BuiltinError` — error boundary belongs to card `250`.
- Extracting `locking`, `cache`, `execute`, `command_context`,
  `tasks_command`, or runner `doctor` into their own crates — future
  lane decisions.
- Reshaping `RunnerError` variants.
- Changes to `runner/manifest.rs` re-exports (card `250` handles the
  swap to direct `effigy_manifest::*` imports).

## Acceptance Criteria

- `src/runner/builtin_ports.rs` exists with `BuiltinRuntimePorts`
  trait + re-exported types; `RunnerBuiltinPorts` implementation
  forwards every method to the existing runner-internal function.
- `try_run_builtin_task` and `BuiltinDispatch::run` accept
  `ports: &dyn BuiltinRuntimePorts`; all builtin-side sites that
  previously reached into `locking`, `cache::{ops,model}`, `execute`,
  `command_context`, `tasks_command`, `doctor`, or
  `deferred_builtins_from_catalogs` now call the port.
- `grep -r "super::super::super::\(locking\|execute\|cache::ops\|cache::model\|command_context\|tasks_command\)\|crate::runner::deferred_builtins_from_catalogs" src/runner/builtin/` returns zero matches.
- `grep -r "super::super::doctor\|super::super::tasks_command\|crate::runner::render::\(plain_renderer\|render_utf8\|text_renderer\|encode_json\)" src/runner/builtin/` returns zero matches.
- `parse_task_selector` lives in `effigy-tasks`; `with_local_node_bin_path`
  is imported directly from `effigy_core::shell` at every site;
  `plain_renderer` / `render_utf8` / `text_renderer` / `encode_json`
  live in `effigy-ui`.
- `src/runner/util.rs` sheds the `parse_task_selector` wrapper (or
  documents why it stays); `src/runner/render/**` sheds the four
  relocated helpers.
- No new crate created. Behavior unchanged — runner still handles
  locking, cache, execution, etc. itself; only the call path
  changed.
- Test totals unchanged: runner lib count plus any unit tests that
  travel with relocated helpers accounted for in the commit message.

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Execution Notes

- `BuiltinRuntimePorts` trait landed at `src/runner/builtin_ports.rs`
  with 13 methods (not 16 as proposed). Three methods from the
  proposal — `check_task_cache`, `update_task_cache_entry`,
  `task_cache_config` — were dropped after grep confirmed no
  `src/runner/builtin/**` site actually consumes them. The cache
  dispatcher only needs `cache_entries`, `cache_entry`,
  `cache_entry_key`, `invalidate_cache_keys`, and
  `invalidate_all_cache_entries`; the cache-hit path during task
  execution lives in `runner::execute`, not builtin.
- `RunnerBuiltinPorts` landed as a zero-sized struct directly in
  `src/runner/builtin_ports.rs` (no separate `runner_impl.rs` split)
  since the impl is a 13-method thin forwarder.
- `LockScope`, `LockGuard`, `UnlockResult`, `TaskCacheEntry` stayed
  as direct type imports from `crate::runner::{locking::model,
  cache::model}::*` at their builtin use sites rather than being
  re-exported through `builtin_ports`. Rationale: card `251` is
  behavior inversion; type-surface exposure is card `250`'s job when
  those types cross the crate boundary.
- `UiError` grew an `Encoding(String)` variant for
  `encode_json`'s `serde_json` error path; `From<UiError> for
  RunnerError` lifts it at the runner boundary. `effigy-cli`'s
  `help/ui.rs` match arm added to stay exhaustive.
- `src/runner/util::parse_task_selector` wrapper deleted; all
  callers (builtin + non-builtin) now route directly through
  `effigy_tasks::parse_task_selector` with
  `From<TaskError> for RunnerError` at the boundary.
- `src/runner/util/shell.rs` re-export shim deleted; every caller
  now imports `effigy_core::shell::with_local_node_bin_path`
  directly.
- `src/runner/render.rs` shrank to just
  `render_task_resolution_trace` + its `trace_renderer` (which now
  delegates to `effigy_ui::text_renderer`). Four relocated helpers
  (`plain_renderer`, `render_utf8`, `text_renderer`, `encode_json`)
  plus `standard_renderer` / `color_enabled_for_text_output` /
  `text_color_enabled` / `resolve_text_color_enabled` now live in
  `crates/effigy-ui/src/output.rs`. Unit tests for the color-mode
  resolver travel with the code.
- Grep gates from Acceptance Criteria all return zero matches. Full
  validation green: `cargo build`, `cargo fmt --check`, `cargo
  clippy` (-D warnings + standard allowlist), `cargo test`.

## Next Task

Execute card [`250-implement-effigy-builtin-extraction.md`](./250-implement-effigy-builtin-extraction.md).
With the port trait in place and the pure helpers relocated, `250`
becomes a mechanical crate move: the trait definition migrates into
`effigy-builtin` as a `pub trait`, the runner provides the
concrete implementation, and every `builtin/**` call site already
routes through the one allowed surface.
