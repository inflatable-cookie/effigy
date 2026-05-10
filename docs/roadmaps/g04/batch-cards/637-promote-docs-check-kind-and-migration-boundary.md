# 637 - Promote Docs Check Kind And Migration Boundary

Lane: [`066-docs-check-subcommand-consolidation-strict-lane.md`](../066-docs-check-subcommand-consolidation-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Goal

Lock the public `docs check <KIND>` boundary before parser and runner changes
start.

## Scope

- lock the bounded docs check kind taxonomy
- lock removed `check-*` spellings and their migration errors
- lock the `docs add-log-index` carveout
- lock the no-behavior-change boundary for the underlying checks

## Acceptance

- the `KIND` set is explicit
- removed spellings have explicit replacement guidance
- `docs add-log-index` remains outside the consolidation
- parser/runner implementation can proceed without reopening the surface

## Result

- the `docs check <KIND>` grammar is locked
- removed `check-*` spellings now have explicit migration wording
- `docs add-log-index` is explicitly out of scope for the consolidation

## Next Task

Execute
[`638-add-docs-check-parser-consolidation.md`](./638-add-docs-check-parser-consolidation.md).
