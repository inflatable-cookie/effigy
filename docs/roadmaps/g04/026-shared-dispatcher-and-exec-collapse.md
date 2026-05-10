# 026 - Shared Dispatcher and Exec Collapse

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-10
Depends on:
- [`025-container-command-decomposition.md`](./025-container-command-decomposition.md)

## Goal

Extract common JSON/text dispatch logic and collapse duplicated `exec_command`
variants. No user-facing behavior changes.

## Scope

### 1. Common JSON/Text Dispatcher

Almost every command repeats:
```rust
if output_json {
    Ok(json_value.to_string())
} else {
    Ok(text_output)
}
```

With error variants:
```rust
if output_json {
    Err(RunnerError::CommandJsonFailure { rendered })
} else {
    Err(RunnerError::task_invocation(text))
}
```

Create a shared helper in `src/runner/common.rs` or `effigy_ui`:
```rust
pub fn render_result(
    output_json: bool,
    success: bool,
    json: serde_json::Value,
    text: String,
) -> Result<String, RunnerError>
```

Apply to commands with heavy duplication:
- `artifact_command`
- `release_command`
- `gateway_command`
- `state_command`
- `contracts_command`
- `distribution_command`
- `docs_command`

### 2. Exec Command Variant Collapse

Four near-identical functions:
- `run_routed_task_container_exec`
- `capture_routed_task_container_exec`
- `run_routed_task_container_exec_with_policy`
- `capture_routed_task_container_exec_with_policy`

Collapse into two:
```rust
fn run_routed_task_container_exec(
    capture: bool,
    policy: Option<ContainerPolicy>,
    ...
) -> Result<..., RunnerError>
```

The two remaining functions handle the policy/no-policy split. Alternatively,
make policy a required parameter and let the caller pass `None`.

### 3. Release Stage Dispatcher

`release_command`'s `Prepare` and `Execute` subcommands repeat the same
`--plan` / `--yes` / interactive branching.

Extract:
```rust
fn run_release_stage(
    stage: ReleaseStage,
    plan: bool,
    yes: bool,
    allow_stale: bool,
    ...
) -> Result<..., RunnerError>
```

## Non-Goals

- No user-facing behavior changes
- No CLI surface changes
- No `.github/workflows/` edits
- No release execution

## Why Now

These are the last major internal duplication hotspots. Collapsing them:
- reduces code volume
- makes behavior changes safer (one place to edit)
- simplifies onboarding for new contributors

## Core Decisions

### Dispatcher Location

`src/runner/render.rs` or `src/runner/common.rs`. It must be accessible from all
command modules without creating circular dependencies.

### Exec Collapse Strategy

Keep the public API stable but delegate to a shared internal function. The four
existing functions become thin wrappers that call the shared implementation with
`capture` and `policy` parameters.

### Release Stage Strategy

`Prepare` and `Execute` are different stages but share the same control-flow
shape. The dispatcher takes a `ReleaseStage` enum and the common flags.

## Success Criteria

- Shared dispatcher used by at least 5 command modules
- Exec command reduced from 4 variants to 2 (or 1)
- Release Prepare/Execute share a common control-flow helper
- All tests pass
- `cargo clippy` passes
- Net line reduction of 200+ lines

## Suggested Batch Order

1. Add shared `render_result` helper
2. Apply to `artifact_command` and `contracts_command` (simplest)
3. Apply to `release_command` and `gateway_command`
4. Apply to `state_command` and `distribution_command`
5. Apply to `docs_command`
6. Collapse exec variants
7. Extract release stage dispatcher

## Validation

- All existing tests pass
- `cargo test` passes
- `cargo clippy` passes
- `git diff --check`
- Net line count reduced

## Next Task

Add shared `render_result` helper.
