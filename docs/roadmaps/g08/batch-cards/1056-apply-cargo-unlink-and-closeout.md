# 1056 - Apply Cargo Unlink And Closeout

Roadmap: [`../020-cargo-local-dependency-linking.md`](../020-cargo-local-dependency-linking.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1055`

## Purpose

Complete reversible Cargo linking: apply owned unlink plans, re-resolve the
committed Git sources without destructive restore, prove clean lock recovery,
and close `g08.020`.

## Owner And Seam

`effigy-deps` owns unlink application, remote-source verification, lock
re-resolution evidence, and cleanup ownership. The runner only dispatches and
renders the shared operation report.

## Work

- apply exact unlink config/ledger deltas and owned empty-directory cleanup
- treat an absent desired link as a successful no-op
- re-resolve affected workspaces after patch removal without Git restore
- verify every formerly linked crate resolves from its committed exact Git
  source and every affected tracked lockfile returns cleanly
- preserve foreign Cargo config and unrelated `.cargo/` contents
- wire text and JSON for `effigy deps unlink cargo <path>` and dry-run
- prove link, edit visibility, re-link, unlink, and clean recovery in flat and
  nested temporary consumers
- close `g08.020` and promote the first Bun card for `g08.021`

## Guardrails

- no `git checkout`, `git restore`, or lockfile replacement from snapshots
- no deletion without recorded Effigy ownership
- no Bun implementation or doctor findings
- no portfolio-repo mutation in fixture validation

## Acceptance

- [x] unlink removes only the selected library's managed state
- [x] foreign config and other linked libraries survive byte-for-byte
- [x] affected crates resolve from committed Git sources after unlink
- [x] tracked lockfiles return cleanly or report exact remaining drift
- [x] missing-library and already-unlinked cases remain safely removable/no-op
- [x] dry-run and JSON report the exact unlink and cleanup plan
- [x] flat and nested round trips pass
- [x] `g08.020` closes with `g08.021` next

## Validation

- focused unlink ownership/no-op/cleanup tests
- temporary flat and nested Cargo round-trip fixtures
- focused CLI text/JSON/dry-run tests
- `effigy qa:ci:json`
- `effigy qa:ci:fast`
- `effigy qa:docs`
- `git diff --check`

## Stop Conditions

Stop and replan if Cargo cannot re-resolve the committed source without
destructive restore, unrelated lock changes cannot be distinguished, or
multiple local libraries cannot be unlinked independently.

## Next Task

Continue active Bun milestone `g08.021` through ready card `1058`.
