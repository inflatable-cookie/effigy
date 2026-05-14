# g05.018 - Task-Like Definition Schema Convergence

Status: Complete
Depends on: `g05.016`

## Goal

Converge duplicated task-like TOML wrappers onto shared `effigy-manifest`
building blocks so compact tasks, bootstrap runs, state hooks, and similar
surfaces stop hand-owning parallel unions.

## Evidence

- `crates/effigy-manifest/src/task_defs.rs` owns a `ManifestTaskDefinition`
  union for normal task tables
- `crates/effigy-manifest/src/config_sections/bootstrap.rs` owns a separate
  `ManifestBootstrapRun` union that resolves to `ManifestTask`
- `src/runner/state_command.rs` owns another `ManifestStateTaskDefinition`
  union for state hook/capture task definitions
- `ManifestInlineTaskDefinition`, `ManifestManagedRun`, and
  `ManifestManagedRunStepTable` already represent the lower-level shared task
  shape, but not every wrapper reuses the same higher-level ref-or-inline shape

## Scope

- identify the common reusable building blocks for selector refs, inline tasks,
  and full task bodies
- move those building blocks into `effigy-manifest` where they belong
- reuse them from bootstrap and state-owned task-like surfaces where the allowed
  syntax is already intended to match
- keep surface-specific restrictions explicit when a call site intentionally does
  not allow the full general task shape

## Non-Goals

- no new task syntax
- no change to which call sites accept selector strings versus inline task bodies
- no change to execution semantics

## Acceptance Criteria

- duplicated task-like wrapper unions are reduced materially
- bootstrap and state task-like surfaces reuse canonical manifest-owned building
  blocks where their schema intent matches
- supported task-like TOML forms remain stable under focused tests

## Completed

- Added manifest-owned `ManifestTaskLikeDefinition` for task-like command,
  sequence, full task, compact inline task, and single-step table shapes.
- Switched `[tasks]` parsing to the canonical task-like definition owner.
- Collapsed `ManifestBootstrapRun` into a transparent wrapper around the
  canonical task-like definition while preserving its public conversion helpers.
- Added manifest-owned `ManifestTaskOrReferenceDefinition` for state surfaces
  that accept either selector references or inline task-like definitions.
- Replaced the runner-private `ManifestStateTaskDefinition` with the
  manifest-owned reference-or-inline owner.
- Preserved state hook/capture default behavior: compact/run/step inline state
  tasks still default to host execution, while full task tables keep their own
  task-level fields.
- Fixed user-global bundle config lookup so `php_app` config keys satisfy
  `php-app` bundle lookups, matching existing tests and historical bundle key
  normalization expectations.

## Validation

- `cargo test -p effigy-manifest single_task_object_without_array_wrapper`
- `cargo test -p effigy-manifest bootstrap_run_accepts_compact_inline_task_run_in`
- `cargo test --lib compact_inline_task_run_in`
- `cargo test -p effigy-manifest -- --test-threads=1`

## Retained Owners

- `ManifestBootstrapRun` remains as a public wrapper type so downstream callers
  keep the bootstrap-specific name and conversion helpers.
- State keeps a tiny runner helper for host-default projection because that is a
  state execution policy, not a generic manifest parsing rule.

## Suggested Validation

- `cargo test -p effigy-manifest`
- focused bootstrap tests
- focused state command tests
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

No next task inside this roadmap. The task-like schema convergence slice is
complete.
