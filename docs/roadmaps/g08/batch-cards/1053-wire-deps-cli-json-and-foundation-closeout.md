# 1053 - Wire Deps CLI JSON And Foundation Closeout

Roadmap: [`../019-dependency-inventory-and-command-foundation.md`](../019-dependency-inventory-and-command-foundation.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Completed: 2026-08-05
Ready after: completed card `1052`

## Purpose

Expose the read-only foundation through the agreed command grammar and stable
JSON contract, then close `g08.019` before manager mutation begins.

## Owner And Seam

`effigy-cli` owns `DepsArgs` parsing/help/global-JSON propagation. The root
runner owns dispatch and text/envelope rendering over `effigy-deps` reports.
Completion and command inventory consume the same registered command surface.

## Work

- add `effigy deps` and `effigy deps status [cargo|bun]`
- parse but do not execute the future `link`/`unlink` grammar and `--dry-run`
  plan shape required by contract `034`
- wire help, command descriptors, global JSON, completion, repo targeting, and
  top-level dispatch
- render bare `deps` and explicit `deps status` from one report path
- add the versioned dependency-status payload under `effigy.command.v1`
- add schema, example, selection-index, help, and CLI output fixtures
- close `g08.019` and point the lane at Cargo milestone `g08.020`

## Guardrails

- link/unlink must remain non-mutating until `g08.020`/`g08.021`
- no doctor integration
- no manager-specific report model in CLI or runner code
- no command aliases outside the contract grammar
- no placeholder dependency commands beyond status, link, and unlink

## Acceptance

- [x] bare deps and explicit status are equivalent
- [x] optional manager filtering is deterministic
- [x] link/unlink dry-run grammar cannot reach mutation
- [x] help, completion, repo targeting, and global JSON are covered
- [x] text and JSON consume one `effigy-deps` report
- [x] JSON schema/index/example checks pass
- [x] `g08.019` closes with `g08.020` as the next milestone

## Validation

- focused CLI parser/help/global-JSON tests
- focused runner deps-command tests
- `effigy qa:ci:json`
- `effigy qa:ci:fast`
- `effigy qa:docs`
- `git diff --check`

## Stop Conditions

Stop and replan if the standard envelope requires a compatibility break, the
CLI must duplicate domain validation, or link/unlink grammar cannot remain
non-mutating until the manager milestones.

## Evidence

- [`../../../logs/2026-08/05-163456-deps-cli-json-foundation-closeout.md`](../../../logs/2026-08/05-163456-deps-cli-json-foundation-closeout.md)

## Next Task

Execute ready batch card `1054`.
