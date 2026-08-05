# 1052 - Add Read-Only Dependency Inventory And Status

Roadmap: [`../019-dependency-inventory-and-command-foundation.md`](../019-dependency-inventory-and-command-foundation.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Completed: 2026-08-05

## Purpose

Populate the shared domain with deterministic, read-only Cargo and Bun
inventory/status evidence before either manager can mutate resolution.

## Owner And Seam

`effigy-deps` owns read-only manager adapters and normalized reports. Process
execution is injected behind narrow ports so fixtures do not require private
portfolio repos or machine-global Bun state.

## Work

- inventory Cargo workspaces and workspace-less multi-crate library layouts
- inventory every Cargo consumer workspace under one repo root
- normalize exact Cargo source URLs and distinguish git, path, registry, and
  unmatched sources
- inventory Bun root/workspace library packages and consumer dependency trees
- match full direct/transitive library closures without applying links
- inspect repo desired state, Cargo managed blocks/resolution, Bun registration
  targets, and consumer symlinks read-only
- produce empty, healthy, missing-path, full-loss, partial-closure, and conflict
  status reports through one manager-neutral model
- add flat, nested-workspace, root-package, and multi-package fixtures

## Guardrails

- no Cargo config, lockfile, ignore-file, Bun registry, symlink, manifest, or
  desired-state writes
- no CLI or doctor integration
- no package-name-only Cargo patch matching without exact source identity
- no partial-closure success result

## Acceptance

- [x] both manager inventories are deterministic and fixture-backed
- [x] nested Cargo consumers and workspace-less libraries are represented
- [x] Bun root and workspace package layouts are represented
- [x] direct/transitive closure and pre-migration path dependencies are distinct
- [x] status distinguishes complete Bun link loss from mixed partial closure
- [x] all external process behavior is injectable and failure is actionable
- [x] inspection performs no writes

## Validation

- focused `effigy-deps` inventory/status tests
- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `effigy qa:docs`
- `git diff --check`

## Evidence

- [`../../../logs/2026-08/05-162005-read-only-dependency-inventory-status.md`](../../../logs/2026-08/05-162005-read-only-dependency-inventory-status.md)

## Stop Conditions

Stop and replan if manager output cannot establish exact source identity or full
closure without mutation, or if nested workspaces invalidate the one-root
Cargo config authority in contract `034`.

## Next Task

Execute ready batch card `1053`.
