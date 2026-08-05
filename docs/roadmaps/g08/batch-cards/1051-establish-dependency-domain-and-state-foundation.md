# 1051 - Establish Dependency Domain And State Foundation

Roadmap: [`../019-dependency-inventory-and-command-foundation.md`](../019-dependency-inventory-and-command-foundation.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Completed: 2026-08-05

## Purpose

Create the low-level dependency-domain and persistence seam before CLI wiring
or package-manager inspection begins.

## Owner And Seam

Add `crates/effigy-deps` as a workspace crate consumed later by the root runner
and `effigy-doctor`. The crate owns dependency-link identities, desired-state
models, state stores, canonical paths, and persistence errors. It must not
depend on `effigy-cli`, `effigy-doctor`, or root-runner modules.

## Work

- add the `effigy-deps` workspace crate and root dependency wiring
- define typed manager, mechanism, link key, library, consumer root, package,
  committed source, desired state, observed state, drift, plan, verification,
  and report models
- define schema-versioned repo state at
  `.effigy/local/dependency-links.json`
- implement deterministic read, empty-state, malformed-state, and atomic-write
  behavior for the repo ledger
- expose an ignore-coverage plan for the repo's `.effigy/` local-state rule;
  do not mutate ignore files in this card
- define schema-versioned machine state at
  `~/.effigy/deps/bun-registrations.json`
- implement an adjacent ownership lock with bounded acquisition and stale-lock
  handling, then atomically update the Bun registration index
- preserve whether a Bun registration was Effigy-created or foreign and retain
  canonical consumer/link references
- use owner-only permissions for newly created machine-global state and lock
  files on Unix
- add focused fixtures for empty, multiple-link, malformed, stale-reference,
  conflicting-registration, and concurrent-update state
- add the new crate and seam rationale to the live package map

## Guardrails

- no CLI grammar, help, completion, or dispatch changes
- no Cargo metadata/process invocation
- no Bun process invocation or symlink mutation
- no doctor integration
- no JSON command schema changes
- no consumer manifest, lockfile, Cargo config, or ignore-file mutation
- no generic persistence framework extraction

## Acceptance

- [x] `effigy-deps` is below CLI, doctor, and runner in dependency direction
- [x] state models serialize deterministically with explicit schema versions
- [x] missing repo state reads as an empty desired-state set
- [x] malformed or future-version state fails actionably without overwrite
- [x] repo-ledger writes replace atomically in the same directory
- [x] Bun registration updates are locked, atomic, and preserve unrelated rows
- [x] stale locks have bounded, tested recovery; live locks fail without hanging
- [x] foreign Bun registrations cannot be claimed by state-model operations
- [x] canonical paths and multiple simultaneous library links round-trip
- [x] package-map ownership is current

## Validation

- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `cargo check -p effigy`
- `effigy qa:docs`
- `git diff --check`

## Evidence

- [`../../../logs/2026-08/05-155727-dependency-domain-state-foundation.md`](../../../logs/2026-08/05-155727-dependency-domain-state-foundation.md)

## Stop Conditions

Stop and replan if:

- the crate needs an upward dependency on `effigy-cli`, `effigy-doctor`, or the
  root package
- safe cross-process state requires a repo-wide locking refactor
- canonical identity cannot distinguish manager, consumer repo, library path,
  and package name without manager-specific mutation
- atomic replacement cannot preserve owner-only machine-state permissions

## Next Task

Execute ready batch card `1052`.
