# 1057 - Plan Bun Full Closure And Registration Ownership

Roadmap: [`../021-bun-local-dependency-linking.md`](../021-bun-local-dependency-linking.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1056`

## Purpose

Turn the read-only Bun inventory and registration index into deterministic,
non-mutating link/unlink plans before any `bun link` process is allowed to
change machine state.

## Owner And Seam

`effigy-deps` owns Bun closure matching, registration observations, ownership
decisions, and operation plans. The runner remains reserved for Bun mutation in
this card.

## Work

- match the full direct/transitive library package closure from existing Bun
  inventory
- model save-less package registration and consumer-link process intents
- snapshot the exact `package.json` and Bun lockfile invariants each later
  apply must preserve
- plan repo-ledger and machine-registration-index deltas atomically
- distinguish absent, matching foreign, Effigy-owned shared, stale, and
  conflicting global registrations
- plan unlink reference release without removing shared or foreign
  registrations
- expose exact dry-run-ready plan models and fixture reports below the CLI
  mutation boundary
- prove root-only and multi-package libraries, closure completeness,
  registration conflicts, and shared-consumer ownership

## Guardrails

- no `bun link`, `bun unlink`, or package-manager mutation
- no `--save`, manifest edit, or lockfile edit
- no Cargo, doctor, or CLI mutation work
- no claim or replacement of foreign global registrations

## Acceptance

- [x] every matched direct/transitive package appears in one complete plan
- [x] no-match and partial/mixed closure outcomes produce no write plan
- [x] matching foreign registrations are usable but never claimed
- [x] conflicting registration paths are refused
- [x] shared Effigy-owned registrations retain exact consumer references
- [x] unlink plans remove only the selected consumer/library references
- [x] manifest and lockfile snapshots are explicit plan invariants
- [x] all planning and dry-run fixtures execute without manager mutation

## Validation

- focused Bun closure and plan fixtures
- registration-index ownership/refcount fixtures
- process-observer tests proving no mutating Bun command runs
- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `git diff --check`

## Stop Conditions

Stop and replan if the installed Bun surface cannot expose registration target
identity without mutation, the consumer closure cannot be established
deterministically, or safe unlink requires claiming foreign state.

## Next Task

Execute ready Bun link-apply card `1058`.
