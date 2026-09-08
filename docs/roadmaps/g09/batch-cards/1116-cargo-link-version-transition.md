# 1116 - Cargo Link Version Transition

Roadmap: [`../008-cargo-link-version-transition.md`](../008-cargo-link-version-transition.md)
Spec: [`../../../specs/archive/123-cargo-link-version-transition-strict-lane.md`](../../../specs/archive/123-cargo-link-version-transition-strict-lane.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete (2026-09-07)
Owner: Cargo link plan, apply transaction, verification, rollback
Created: 2026-09-07
Active since: 2026-09-07 operator-authorised handoff
(`docs/handoffs/20260907-cargo-version-transition-link.md`), branch
`fix/cargo-version-transition-link`, base `729aa89c`
Merged: 2026-09-07 at `7d9c8be`

## Purpose

Make `deps link cargo` cross a local package version bump and clean up
completely when it fails.

## Work

- detect the per-package version transition at plan time and report it
- snapshot every affected `Cargo.lock` before the first write; restore with
  config on failure
- refresh only affected linked packages inside the transaction, after the
  patch write, before verification
- fail verification by name on any `[[patch.unused]]` for a linked package
- add the real `v0.4.3` to `v0.4.4` fixture and the failed-verification
  rollback regression
- update contract `034` lockfile-safety and guide `077`; reconcile the
  operator's `PAPERCUTS.md` entry; `CHANGELOG.md` `[Unreleased]` **Fixed**;
  one evidence log

## Acceptance

- [x] transition fixture links every matched package from the local path;
      no `[[patch.unused]]`
- [x] forced verification failure restores config and every affected
      `Cargo.lock` byte-for-byte
- [x] unrelated lockfiles and unlinked packages' entries unchanged
- [x] unlink still returns byte-for-byte; dirty-lock refusal unchanged
- [x] contract `034` and guide `077` record the transition case

## Review Oracle

Falsify before PR creation: (1) fixture still unused/Git; (2) residue after
failure; (3) unrelated lock change; (4) refresh wider than affected
packages; (5) unlink or dirty-lock rule weakened; (6) docs not updated.

## Validation

- focused `effigy-deps` tests, the transition fixture, the rollback regression
- `effigy graph affected` for changed source, then direct targets
- `effigy qa` or targeted QA as the handoff allows
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `git diff --check`

## Evidence Requirement

One dated closeout log under `docs/logs/archive/2026-09/` mapping every oracle row
to proof, with the fixture output before and after.

## Stop Conditions

Per spec `123`.

## Evidence

[`07-162725-cargo-link-version-transition-1116`](../../../logs/archive/2026-09/07-162725-cargo-link-version-transition-1116.md) maps every oracle row
and acceptance line to proof, with the fixture output before and after.

## Next Task

Merged at `7d9c8be`. Card `1117` is the final `g09` lane.
