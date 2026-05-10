# 053 - Contract Promotion And g04 Closeout Strict Lane

Roadmap: [`g04.011`](../roadmaps/g04/011-contract-promotion-and-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Promote the shipped `g04` runtime/container architecture into durable
contracts and package-map docs, then close the generation cleanly.

## Hard Boundaries

- preserve public CLI behavior unless a card explicitly documents a cleanup
  break
- add changelog entries only for public behavior changes
- no release work
- no `.github/workflows/` edits
- no broad docs rewrites outside the contracts/package-map closeout surface

## Current Ready Card

None. This lane is complete.

## Execution Chain

- `572` complete: close drift guards and hand off to contract promotion
- `573` complete: scaffold contract promotion closeout lane
- `574` complete: promote `g04` crates into package map
- `575` complete: add runtime operation pipeline contract
- `576` complete: align existing contracts with runtime operation pipelines
- `577` complete: close `g04` contract promotion

## Promotion Targets

- `docs/architecture/010-package-map.md`
- `docs/contracts/005-container-runtime-contract.md`
- `docs/contracts/009-execution-surface-convergence.md`
- `docs/contracts/012-container-manager-contract.md`
- `docs/contracts/013-task-execution-request-contract.md`
- `docs/contracts/014-artifact-substrate-contract.md`
- `docs/contracts/015-runtime-operation-pipeline-contract.md`

## Decisions To Make

- whether a new `015` contract is needed, or whether existing contracts can
  carry the runtime operation pipeline rules
- which drift-guard allowances are adapter boundaries versus migration debt
- whether any public behavior changed during `g04` and needs changelog notes
- whether `g04` can close after contract promotion or needs a final cleanup
  roadmap

## Promotion Inventory

### Package Map Drift

`docs/architecture/010-package-map.md` names the older `g03` core seams but is
missing several shipped `g04` ownership boundaries:

- `effigy-runtime-plan`: runtime activation request/plan/report substrate
- `effigy-containers`: typed container operation request/plan/report model
- `effigy-data`: seed/dump target, artifact handoff, and database command
  planning
- `effigy-artifacts`: artifact refs, OCI adapter, staging, apply, and capture
  substrate

The map also still describes some runner/runtime files as broad owners where
`g04` split the planning substrate into crates and stage modules.

### Contract Drift

- `005-container-runtime-contract.md` needs to name `effigy-runtime-plan` as
  the runtime activation planning substrate and keep runner/runtime prep as the
  side-effect adapter.
- `009-execution-surface-convergence.md` needs updated ownership rows for
  runtime activation, container operations, data seed/dump, and Rhai
  `exec::run(...)`.
- `012-container-manager-contract.md` needs to acknowledge the current
  manager-plan and drift-guard state without pretending all compatibility
  wrappers are gone.
- `013-task-execution-request-contract.md` is broadly current, but should point
  to the newer proof matrix and Rhai stdin/container proof.
- `014-artifact-substrate-contract.md` should name `effigy-data` as the seed
  and dump planning owner, with `effigy-artifacts` owning transport/staging.
- `015-runtime-operation-pipeline-contract.md` now carries the four pipeline
  families that cut across existing contracts.

### Changelog/Public Behavior

No public CLI behavior change has been identified in this closeout inventory.
Treat changelog work as unnecessary unless a later contract-promotion card
finds an intentional public cleanup break.

## Selected First Promotion Slice

Card `574` updated the package map first. That gives the remaining contract
cards a current owner map to reference.

## Selected Contract Slice

Card `575` added `015-runtime-operation-pipeline-contract.md` because the
pipeline rules cut across container runtime, execution convergence, container
manager, task execution, and artifact/data contracts.

## Selected Existing-Contract Slice

Card `576` updated the existing contracts to reference `015` and current crate
owners without broad rewrites.

## Selected Closeout Slice

Card `577` should close `g04.011`, mark this lane complete, and leave the next
move explicit. No public behavior change was identified during contract
promotion, so no changelog entry is expected.

## Exit Condition

This lane closes when the package map and contracts name current owners, any
public behavior changes are reflected in changelog/docs, no stale ready card
remains, and the next move is explicit.

## Next Task

Planning stop. The current `g04` roadmap set is complete; add the next `g04`
roadmap only by explicit human request.
