# 650 - Close Container Command Thin Dispatcher

Roadmap: [`../025-container-command-decomposition.md`](../025-container-command-decomposition.md)
Strict lane: [`../../../specs/068-container-command-decomposition-strict-lane.md`](../../../specs/068-container-command-decomposition-strict-lane.md)
Contract: [`../../../contracts/023-container-command-decomposition-contract.md`](../../../contracts/023-container-command-decomposition-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Updated: 2026-05-10

## Purpose

Finish the container-command decomposition lane by pushing the remaining family
dispatch out of `mod.rs` and closing the thin-dispatcher target.

## Scope

- move the remaining `data` subcommand dispatch into `data.rs`
- remove now-redundant data adapters from `mod.rs`
- keep `mod.rs` as top-level dispatch glue plus tiny shared seams only
- confirm the lane target is met with focused container proof coverage

## Acceptance

- `mod.rs` is reduced to thin dispatch shape
- data-family dispatch now lives with the data owner
- the lane can close without opening new behavioral debt
