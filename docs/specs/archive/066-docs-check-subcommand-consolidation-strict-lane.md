# 066 - Docs Check Subcommand Consolidation Strict Lane

Roadmap: [`g04.023`](../roadmaps/g04/023-docs-check-subcommand-consolidation.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Purpose

Collapse the flat `docs check-*` subcommand surface into one `docs check
<KIND>` dispatcher without changing the underlying check behavior.

## Hard Boundaries

- do not change docs-check logic
- do not change docs-check JSON schemas
- keep `docs add-log-index` unchanged
- no `.github/workflows/` edits
- no release execution

## Current Ready Card

- none; lane complete

## Execution Chain

- `636` complete: opened the lane, promoted the contract anchor, and selected
  the first contract-boundary card
- `637` complete: locked the kind taxonomy, removed-spelling migration errors,
  `add-log-index` carveout, and no-behavior-change rule for the underlying
  checks
- `638` complete: collapsed the docs parser surface into `docs check <KIND>`,
  updated the typed CLI shape to one docs-check variant, and landed parser/help
  proofs plus migration-error coverage
- `639` complete: finished the broad runner/docs/completion migration, updated
  live task/starter surfaces, and closed the lane

## Exit Condition

This lane is complete when all docs checks run through `effigy docs check
<KIND>`, the old `check-*` spellings are gone with migration errors, and the
visible help/reference surfaces reflect the new shape.

## Next Task

Open the next queued `g04` lane.
