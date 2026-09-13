# g10 — Agent-Native Skill Execution

Status: active
Opened: 2026-09-13
Owner: task routing and execution

## Generation intent

Make installed agent skills first-class Effigy task sources without weakening
the explicit source/consumer split or machine-safe output contracts.

## Approved frontier

- [`g10.001`](./001-named-skill-resolution-and-stdio-passthrough.md) — ready;
  resolve qualified installed skills by name and add opt-in raw stdio transport.

No later task is approved. Return to Chatterbox after `g10.001` closes.

## Boundaries

- Existing explicit `--path`, human output, and JSON envelope behavior stay
  compatible.
- Skill lookup is local and deterministic. No registry or network acquisition.
- Consumer catalogs and runtime configuration remain isolated.
- Release and workflow mutations remain separately gated.

## Next Task

Dispatch `g10.001` through Northstar Queue from its committed handoff.
