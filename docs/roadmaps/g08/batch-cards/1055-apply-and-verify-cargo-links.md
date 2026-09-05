# 1055 - Apply And Verify Cargo Links

Roadmap: [`../020-cargo-local-dependency-linking.md`](../020-cargo-local-dependency-linking.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Completed: 2026-08-05
Ready after: completed card `1054`

## Purpose

Apply the proven Cargo link plan, verify the full local closure, and expose
`effigy deps link cargo` without weakening dry-run or committed-source safety.

## Owner And Seam

`effigy-deps` owns plan precondition checks, bounded file application, Cargo
verification, and operation reports. The root runner resolves command context
and renders the shared report. It must not write config or infer closure.

## Work

- compose library/consumer inventories and the `1054` plan behind one Cargo
  link operation
- re-check every planned file's exact `before` state before mutation
- apply config and ignore deltas atomically; persist desired ledger state only
  after full-closure verification passes
- verify every matched crate resolves from its canonical local package path
  through Cargo metadata/tree evidence across flat and nested workspaces
- keep `--dry-run` on the same operation path while performing zero writes or
  verification commands
- handle already-linked plans as verified idempotent refreshes
- report per-crate committed source, planned local source, observed source,
  affected lockfiles, warnings, and verification verdict
- wire text and standard-envelope JSON for `effigy deps link cargo <path>`
- add bounded rollback for Effigy-applied config/ignore changes when verification
  fails; never use Git restore commands

## Guardrails

- no Cargo unlink mutation
- no manifest edits or global Cargo config
- no partial-closure success
- no ledger persistence before verification
- no destructive Git restore
- no Bun behavior or doctor integration

## Acceptance

- [x] dry-run renders the exact `1054` plan and performs no writes/process verify
- [x] apply refuses stale plan preconditions before its first write
- [x] flat and nested workspaces resolve the complete closure locally
- [x] verification failure is explicit and rolls back only Effigy-applied files
- [x] successful apply persists desired state after verification
- [x] re-link is idempotent and repairs a missing owned block
- [x] text and JSON share one operation report
- [x] affected lockfile warnings remain prominent and machine-readable

## Validation

- focused apply/precondition/rollback tests
- temporary flat and nested Cargo git-dependency fixtures
- focused CLI text/JSON/dry-run tests
- `cargo test -p effigy-deps`
- `effigy qa:ci:json`
- `effigy qa:ci:fast`
- `git diff --check`

## Stop Conditions

Stop and replan if verification cannot prove canonical local paths for every
matched crate, safe rollback would require discarding unrelated lock changes,
or the CLI must reconstruct manager policy outside `effigy-deps`.

## Evidence

- [`../../../logs/archive/2026-08/05-172006-cargo-link-apply-verification.md`](../../../logs/archive/2026-08/05-172006-cargo-link-apply-verification.md)

## Next Task

Execute ready Cargo unlink/closeout card `1056`.
