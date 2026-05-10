# 646 - Extract Container Cache Command Family

Roadmap: [`../025-container-command-decomposition.md`](../025-container-command-decomposition.md)
Strict lane: [`../../../specs/068-container-command-decomposition-strict-lane.md`](../../../specs/068-container-command-decomposition-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Move `container cache list` and `container cache prune` out of
`container_command/mod.rs` into one focused `cache.rs` owner.

## Scope

- extract cache list/prune entrypoints into `cache.rs`
- move cache-specific helper logic out of `mod.rs` where possible
- keep output and validation behavior unchanged
- leave volume, lifecycle, and broader shared-helper cleanup for later cards

## Acceptance

- `cache.rs` owns cache list/prune entrypoints
- `mod.rs` shrinks and keeps only dispatch glue for cache commands
- focused container cache proofs stay green
