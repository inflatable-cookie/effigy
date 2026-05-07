# 387 - Inventory Runtime Container Cleanup Targets

Lane: [`039-runtime-container-caller-migration-and-cleanup-strict-lane.md`](../039-runtime-container-caller-migration-and-cleanup-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Inventory the remaining runtime/container caller cleanup targets after
`g03.030`, `g03.031`, and `g03.032`, then choose the first implementation card.

## Scope

- inventory direct cwd/root probes in runner code
- inventory runner-local container backend branching
- inventory task execution paths that still bypass `TaskExecutionRequestBuilder`
- inventory remaining oversized mixed-ownership runtime/container files
- choose a first implementation card with a narrow write set
- do not make runtime behavior changes in this card

## Exit Condition

This card is complete when the cleanup targets are ranked and the next ready
card has a clear migration/write boundary.

## Inventory

### Rank 1 - Runner Cwd/Root Compatibility Wrappers

Remaining command-local cwd/root wrapper callers:

- `src/runner/system_command.rs`
- `src/runner/service_command.rs`
- `src/runner/distribution_command/mod.rs`
- `src/runner/exec_command/mod.rs`
- `src/runner/deploy_command/mod.rs`
- `src/runner/release_command/mod.rs`
- `src/runner/system_command/workspace/mod.rs`
- `src/runner/defer_command.rs`
- `src/runner/contracts_command.rs`
- `src/runner/tasks_command/prepare.rs`
- `src/runner/docs_command/mod.rs`
- `src/runner/container_command/mod.rs`
- `src/runner/demo_command/entry.rs`
- `src/runner/execute/preflight/context/discovery.rs`
- `src/runner/system_command/workspace_provisioning.rs`

These wrappers are context-backed, so behavior is already safer, but the
callers still preserve old local resolution shape.

### Rank 2 - Task Execution Request Follow-Through

`TaskExecutionRequestBuilder` is used by:

- direct dispatch
- execute API
- Rhai `exec::run(...)`

Remaining follow-through is mostly internal policy bridging:

- `container_runtime_prep::ExecutionSurfaceKind`
- standard task activation
- deferral activation
- bootstrap handoff activation
- explicit exec activation

### Rank 3 - Container Backend Compatibility Wrappers

Runner-level drift check is clean:

```bash
rg "resolve_compose_backend|ComposeBackend" \
  src/runner/exec_command \
  src/runner/container_command \
  crates/effigy-runtime/src/write.rs -n
```

Remaining direct backend branching is lower-level:

- `crates/effigy-containers/src/exec.rs`
- `crates/effigy-containers/src/colima.rs`
- `crates/effigy-containers/src/lib.rs`
- `crates/effigy-containers/src/compose.rs`

That is acceptable as a temporary compatibility boundary from `g03.031`, but
should be shrunk after runner callers stop depending on legacy wrappers.

### Rank 4 - Mixed-Ownership File Hotspots

Largest cleanup targets:

- `crates/effigy-containers/src/exec.rs` - 1616 lines
- `crates/effigy-containers/src/workspace.rs` - 1541 lines
- `crates/effigy-containers/src/lib.rs` - 1506 lines
- `crates/effigy-containers/src/policy_support.rs` - 1306 lines
- `src/runner/bootstrap_command/mod.rs` - 1083 lines
- `src/runner/script_command/mod.rs` - 1078 lines
- `src/runner/execute/pipeline/managed.rs` - 854 lines
- `src/runner/container_command/lifecycle.rs` - 775 lines
- `src/runner/execute/pipeline/standard.rs` - 729 lines
- `src/runner/container_command/support.rs` - 723 lines

## Decision

Start with Rank 1. It is the narrowest cleanup that directly advances the
runtime-context contract and reduces old caller-local path shape without
changing public behavior.

## Next Task

Implement card `388`: migrate simple runner command cwd/root callers to an
explicit runtime context helper.
