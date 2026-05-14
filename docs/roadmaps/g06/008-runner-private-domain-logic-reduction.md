# g06.008 - Runner-Private Domain Logic Reduction

Status: Complete
Depends on: `g06.001`

## Goal

Move durable domain logic out of runner command modules when that logic is no
longer specific to CLI parsing or final rendering.

## Evidence

- a recurring Effigy pattern is runner command modules owning parsing,
  validation, planning, execution policy, report shaping, and rendering in one
  place
- this inflates command files and creates repeated domain logic across adjacent
  surfaces
- provider, state, demo, and release work all showed this pattern during
  `g05`

## Scope

- identify runner modules that still own domain logic better housed in a crate
  or domain module
- move only durable non-CLI behavior
- reduce parallel planning/validation/reporting codepaths inside command
  modules
- keep CLI adaptation and final command dispatch in runner

## Out Of Scope

- no blanket migration of every helper out of runner
- no crate explosion for tiny moves
- no abstract service layer invention
- no contract changes unless separately justified

## Guardrails For A Cheaper Model

- move logic only when another owner is clearly more durable
- keep the runner explicit and readable
- avoid creating "util" graveyards
- preserve current diagnostics and error messages unless tests intentionally
  update them

## Suggested Implementation Steps

1. Inventory command modules with mixed responsibilities.
2. Classify helpers as CLI-only, domain-owned, or rendering-owned.
3. Move the clearest domain-owned helpers first.
4. Add focused tests at the new owner boundary.
5. Leave borderline helpers in place and document why.

## Acceptance Criteria

- runner command modules own less durable business logic
- moved logic has clearer homes
- command modules become thinner without becoming opaque
- retained runner-private logic is explicitly justified

## Current State

- `state_command.rs` was the primary active target and is no longer a
  warning-level god file
- state-domain IO, context-file writing, env construction, skip-layer
  validation, history-kind parsing, composed-state loading, and named
  capture-profile request expansion now live under `effigy-state`
- the remaining runner surface is now much closer to CLI adaptation,
  execution dispatch, and final rendering

## Validation

Focused validation depends on touched modules. Always include:

```bash
cargo test
effigy scan god-files --json
```

## Next Task

Completed. Next closeout lane is `809`.
