# 1058 - Apply And Verify Bun Links

Roadmap: [`../021-bun-local-dependency-linking.md`](../021-bun-local-dependency-linking.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1057`

## Purpose

Apply the complete Bun link plan with explicit save suppression, verify every
consumer symlink, and prove committed package manifests and lockfiles remain
byte-for-byte unchanged.

## Owner And Seam

`effigy-deps` owns Bun process application, exact preconditions, immutable-file
guards, rollback, and full-closure verification. The runner only dispatches and
renders shared operation reports.

## Work

- re-check exact ledger, index, ignore, registration, and consumer-link
  preconditions before the first mutation
- coordinate registration ownership through the locked machine-local index
- run every planned registration and consumer link with explicit `--no-save`
- apply only a complete consumer closure; rollback created links and owned
  registrations on process, invariant, verification, or persistence failure
- compare `package.json`, `bun.lock`, and `bun.lockb` against the exact plan
  snapshots after every manager mutation
- verify every consumer symlink resolves to its canonical local package path
- make re-link repair complete symlink loss without duplicating state
- wire text and JSON for `effigy deps link bun <path>` and dry-run
- prove real save-less behavior against supported Bun `1.3.14`

## Guardrails

- no process invocation without explicit `--no-save`
- no manifest or lockfile mutation tolerance
- no partial-closure apply or foreign registration replacement
- no Bun unlink, peer-dedupe, doctor, or portfolio mutation work
- no state persistence before full physical verification

## Acceptance

- [x] dry-run executes no Bun process or state mutation
- [x] every matched package is registered/linked or the operation rolls back
- [x] foreign matching registrations remain foreign and untouched
- [x] stale owned registrations and complete consumer-link loss are repaired
- [x] manifest and lockfile bytes remain identical across success and failure
- [x] verification records the canonical target for every package
- [x] state/index conflicts discovered after planning fail before mutation
- [x] text and JSON expose exact intents, outcomes, verification, and rollback

## Validation

- focused apply/precondition/rollback fixtures
- functional root-only and multi-package `bun link --no-save` fixtures
- re-link-after-install drift fixture
- focused CLI text/JSON/dry-run tests
- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `git diff --check`

## Stop Conditions

Stop and replan if supported Bun mutates a manifest or lockfile despite
explicit `--no-save`, exact foreign/owned registration identity cannot be
revalidated under the index lock, or failure rollback would remove foreign or
shared machine state.

## Next Task

Execute ready Bun unlink and milestone-closeout card `1059`.
