# g07.054 - Noninteractive Init Action Execution And Migration Paths

Status: Complete
Depends on: `g07.053`

## Goal

Let non-interactive init cover the same setup surface as the TTY wizard through
explicit checklist-driven execution rather than hidden prompt logic.

## Scope

- define how callers select checklist actions non-interactively
- support an agent-friendly path such as:
  - `effigy init --checklist --json`
  - `effigy init --apply-actions <...>`
- preserve current `--check`, `--apply`, and `--repair` semantics for baseline
  setup
- add deterministic execution ordering for multi-action runs
- add migration-path support for package-script cleanup and task-surface
  normalization where the adapter layer proves it is safe

## Guardrails

- no hidden default widening in CI/non-TTY usage
- checklist execution must be explicit about which actions are selected
- action names must stay stable and scriptable
- partial failures must report per-action outcome
- existing baseline init behavior must remain available without needing the
  full checklist surface

## Acceptance Criteria

- an agent can retrieve a checklist plan and execute selected actions without
  TTY prompts
- non-interactive init can drive graph, secrets, bundle, task-migration, and
  validation setup when relevant
- execution reports clearly separate applied, skipped, blocked, and
  inspection-only actions

## Next Task

Execute `1005`.
