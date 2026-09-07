# g09.008 Cargo Link Version Transition

Status: Active (dispatched 2026-09-07 on direct operator authorisation)
Created: 2026-09-07
Spec: [`123`](../../specs/123-cargo-link-version-transition-strict-lane.md)
Card: [`1116`](./batch-cards/1116-cargo-link-version-transition.md)
Contract: [`034`](../../contracts/034-local-dependency-linking-contract.md)
Guide: [`077`](../../guides/077-local-dependency-linking.md)
Origin: Swallowtail Chatterbox request on behalf of Bovine Desktop
(2026-09-07); operator-authorised handoff
`docs/handoffs/20260907-cargo-version-transition-link.md` at base
`729aa89c`, branch `fix/cargo-version-transition-link`

## Purpose

`effigy deps link cargo` works across a local package version bump: the
affected locked packages are refreshed before verification, and a failed
link restores every affected `Cargo.lock` byte-for-byte with the config.

## Problem

Bovine Desktop pins eight Swallowtail crates at Git tag `v0.4.3`. Linking
local candidates at `0.4.4` writes `[patch]` entries Cargo cannot apply to
the locked `0.4.3` entries, so all eight land as `[[patch.unused]]`,
verification correctly fails, config is rolled back, and `Cargo.lock` keeps
the residue. Verified in code: verification runs `cargo metadata` unlocked
and can rewrite the lock; rollback restores only config paths; the plan never
compares the local version with the locked source version.

## Decision

- Refresh only the affected linked packages across the version transition,
  inside the existing link transaction, before metadata verification. No
  global `cargo update`, no broad lockfile rewrite.
- Every affected lockfile joins the transactional rollback so failed
  verification restores the exact baseline.
- A real `v0.4.3` to `v0.4.4` fixture and a failed-verification rollback
  regression prove it. Unrelated lockfiles and existing link behaviour are
  unchanged. No Desktop pin or Swallowtail manifest changes.

## Cards

- [ ] [`1116`](./batch-cards/1116-cargo-link-version-transition.md) — active

## Acceptance

- linking a higher local version over a tag-pinned Git dependency resolves
  every matched package from the local path with no `[[patch.unused]]`
- a forced verification failure leaves config and every affected
  `Cargo.lock` byte-for-byte at baseline
- unrelated lockfiles are untouched; `deps status cargo` and `unlink` keep
  their contract `034` guarantees
- contract `034` lockfile-safety and guide `077` record the transition case

## Non-Goals

- `cargo update --workspace` or any refresh beyond affected packages
- release, workflow, or consumer-repository mutation
- Bun linking changes

## Dispatch Manifest

Published for the coordinator at the promoting commit. This lane was already
dispatched from the operator-authorised handoff above; the manifest records
authority and closeout surfaces and does not relaunch anything.

- **Lane:** card `1116`, roadmap `g09.008`, strict spec `123`. State: active
  on branch `fix/cargo-version-transition-link` (base `729aa89c`).
- **Prerequisites:** none further. **Completion:** PR merged with evidence
  log, card, roadmap, spec, contract `034`, guide `077`, changelog, and the
  operator's `PAPERCUTS.md` entry reconciled.
- **Owned mutable paths:** `crates/effigy-deps/src/**`, its tests, deps
  fixtures under `tests/fixtures/**`, `docs/guides/077-local-dependency-linking.md`,
  `PAPERCUTS.md` (the named entry only).
  **Reserved shared closeout surfaces:** `CHANGELOG.md` `[Unreleased]`,
  `docs/logs/2026-09/`, `docs/logs/README.md`, this roadmap, card `1116`,
  spec `123`, contract `034` lockfile-safety section, `docs/specs/README.md`,
  `docs/roadmaps/README.md`, `docs/roadmaps/g09/README.md`.
- **Concurrency:** no approved siblings; no serial edges.
- **Worker capability class:** as dispatched by the coordinator.
- **Acceptance evidence and review oracle:** card `1116` acceptance and spec
  `123` oracle; focused deps tests, the transition fixture, the rollback
  regression, `effigy qa` or targeted QA, fmt, clippy, `git diff --check`;
  one dated evidence log.
- **Stop conditions and escalation owner:** spec `123` stop conditions;
  planning questions to the coordinator, then Chatterbox.

## Next Task

Execute card `1116`; on merge, tell the Swallowtail Chatterbox the fix is on
`main` and delete the handoff per the closeout rule.
