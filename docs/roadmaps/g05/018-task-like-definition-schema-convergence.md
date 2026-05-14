# g05.018 - Task-Like Definition Schema Convergence

Status: Planned
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

## Suggested Validation

- `cargo test -p effigy-manifest`
- focused bootstrap tests
- focused state command tests
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Open the implementation lane for canonical task-like definition building blocks
and bootstrap/state adoption.
