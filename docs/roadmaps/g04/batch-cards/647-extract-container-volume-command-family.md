# 647 - Extract Container Volume Command Family

Roadmap: [`../025-container-command-decomposition.md`](../025-container-command-decomposition.md)
Strict lane: [`../../../specs/068-container-command-decomposition-strict-lane.md`](../../../specs/068-container-command-decomposition-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Purpose

Move `container volume list` and `container volume prune` out of
`container_command/mod.rs` into one focused `volume.rs` owner.

## Scope

- extract volume list/prune entrypoints into `volume.rs`
- move volume-specific helper logic out of `mod.rs` where possible
- keep output and validation behavior unchanged
- leave lifecycle and broader shared-helper cleanup for later cards

## Acceptance

- `volume.rs` owns volume list/prune entrypoints
- `mod.rs` shrinks again and keeps only dispatch glue for volume commands
- focused container volume proofs stay green
