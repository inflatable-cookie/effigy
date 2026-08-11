# 104 - Bun Committed Dependency Pinning

Status: Complete
Owner: Platform
Created: 2026-08-11
Promoted: 2026-08-11
Completed: 2026-08-11
Roadmap: [`g08.031`](../../roadmaps/g08/031-bun-committed-dependency-pinning.md)
Contract: [`040`](../../contracts/040-bun-committed-dependency-pinning-contract.md)

## Purpose

Preserve the planning decision that committed Bun overrides need a different
command contract from machine-local links.

Durable authority now lives in:

- [`architecture 023`](../../architecture/023-local-dependency-linking-architecture.md)
- [`contract 040`](../../contracts/040-bun-committed-dependency-pinning-contract.md)
- [`contract 034`](../../contracts/034-local-dependency-linking-contract.md) for
  the unchanged machine-local link boundary

## Settled Direction

- `deps pin bun` and `deps unpin bun` author only root-consumer overrides.
- Pinning is committed state; linking stays ephemeral, save-less, and invisible
  to Git.
- The pin covers the full matched direct-and-transitive library package
  closure or writes nothing.
- Relative `file:` values are portable only when CI and teammates reproduce
  the checkout topology; absolute committed paths are forbidden.
- Pin/unpin never runs install, edits a lockfile, mutates another repository,
  or participates in link ownership state.
- Exact conflict, atomicity, formatting-preservation, and link-interaction
  rules live only in contract `040`.

## Promotion State

The design review is complete. Durable behavior is promoted; this file carries
only strict-lane state.

## Lane Posture

Posture: `strict-complete`

Cards:

- [`1078`](../../roadmaps/g08/batch-cards/1078-build-bun-pin-planner-and-manifest-transaction.md) — complete
- [`1079`](../../roadmaps/g08/batch-cards/1079-wire-bun-pin-cli-json-and-link-interlocks.md) — complete
- [`1080`](../../roadmaps/g08/batch-cards/1080-prove-bun-pin-consumer-workflow-and-closeout.md) — complete

## Stop Conditions Carried Forward

Return to design if implementation would require hidden unpin ownership,
whole-manifest churn, partial closure, automatic install, or mutation outside
the selected consumer manifest.

## Evidence

- [`11-234531-bun-pin-consumer-proof-and-closeout.md`](../../logs/2026-08/11-234531-bun-pin-consumer-proof-and-closeout.md)

## Next Task

Lane complete. Contract `040` owns the durable behavior.
