# g10 — Agent-Native Skill Execution

Status: active
Opened: 2026-09-13
Owner: task routing and execution

## Generation intent

Make installed agent skills first-class Effigy task sources without weakening
the explicit source/consumer split or machine-safe output contracts.

## Approved frontier

- [`g10.001`](./001-named-skill-resolution-and-stdio-passthrough.md) — complete;
  resolve qualified installed skills by name and add opt-in raw stdio transport.

No later task is approved. Return to Chatterbox for planning direction.

## Boundaries

- Existing explicit `--path`, human output, and JSON envelope behavior stay
  compatible.
- Skill lookup is local and deterministic. No registry or network acquisition.
- Consumer catalogs and runtime configuration remain isolated.
- Release and workflow mutations remain separately gated.

## Queue lifecycle adoption

- [g10.002 Effigy-hosted lifecycle hook](002-adopt-effigy-hosted-lifecycle-hook.md)
  is an operator-approved, configuration-only maintenance lane. It follows its
  declared Queue dependencies and may run without changing product priority.
  Existing next-task text continues to describe product sequencing; this entry
  authorizes no sibling product work.

## Next Task

Return to Chatterbox for planning direction. Do not compile `g10.002` without
operator direction.
