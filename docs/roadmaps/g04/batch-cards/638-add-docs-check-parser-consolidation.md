# 638 - Add Docs Check Parser Consolidation

Lane: [`066-docs-check-subcommand-consolidation-strict-lane.md`](../066-docs-check-subcommand-consolidation-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Goal

Replace the flat docs-check parser variants with one `docs check <KIND>`
surface and the locked migration errors for removed spellings.

## Scope

- add `DocsCheckKind`
- collapse docs parser variants into one `Check { kind, ... }` shape
- keep `add-log-index` unchanged
- reject removed `docs check-*` spellings with the locked migration wording
- update docs-command help and parser tests

## Acceptance

- every supported docs check kind parses under `docs check <KIND>`
- removed spellings fail with replacement guidance
- `docs add-log-index` still parses unchanged
- parser/help tests cover the new surface

## Result

- docs parser now routes through `docs check <KIND>`
- the flat docs-check CLI shape is collapsed into one typed check variant
- removed `check-*` spellings now fail with migration guidance

## Next Task

Execute
[`639-close-docs-check-runner-docs-and-completions.md`](./639-close-docs-check-runner-docs-and-completions.md).
