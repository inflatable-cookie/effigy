# 488 - Wire Manager Compose Plan Into Interactive Shell

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move interactive and command shell execution through manager-owned compose
invocation plans.

## Scope

- wire runtime shell command execution through `ContainerComposeInvocationPlan`
- wire runtime shell user probe through the same plan path
- preserve shell selection, workspace identity, color env, working-dir, and
  handoff env behavior
- keep captured exec behavior from card `487`

## Non-Goals

- no attached managed-session migration yet
- no data/cache migration yet
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runtime shell callers no longer call lower-level
compose exec helpers directly.

## Closeout

Runtime shell execution now builds manager-owned compose invocation plans for:

- workspace-user probes
- non-interactive shell commands
- interactive shell handoff
- workspace shell handoff

The migration preserves shell selection, workspace identity, color env,
working-dir handling, handoff env injection, and existing Colima direct exec
behavior.

## Validation

- `cargo test -p effigy --lib container_command`
- `cargo test -p effigy-rhai`
- `cargo test -p effigy --lib workspace`
- `git diff --check`

## Next Task

Select attached session or data/cache manager migration.
