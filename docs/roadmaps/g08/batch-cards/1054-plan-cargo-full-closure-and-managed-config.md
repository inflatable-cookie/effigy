# 1054 - Plan Cargo Full Closure And Managed Config

Roadmap: [`../020-cargo-local-dependency-linking.md`](../020-cargo-local-dependency-linking.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Completed: 2026-08-05
Ready after: completed card `1053`

## Purpose

Turn the proven Cargo inventories into a complete, collision-safe link/unlink
plan with exact managed config and ignore deltas before process mutation or CLI
apply is enabled.

## Owner And Seam

`effigy-deps` owns Cargo closure selection, safety inspection, and pure file
transforms. Git and filesystem observations enter through narrow read-only
ports. The runner remains a renderer and must not reconstruct Cargo policy.

## Work

- group every matching direct/transitive library crate by its exact declared
  git source URL across all consumer workspaces
- reject no-match, registry-only, unmatched, and pre-migration path outcomes
  without producing a write plan
- require the complete matching closure; never expose a partial-link plan
- plan one repo-root `.cargo/config.toml` using canonical absolute package paths
- add/remove one clearly delimited block per library while preserving unrelated
  TOML, comments, and hand-managed entries
- refuse tracked local config, malformed managed markers, and same-source/crate
  foreign patch collisions
- plan `.cargo/config.toml` ignore coverage and Effigy-owned empty-file cleanup
- discover affected tracked `Cargo.lock` files and refuse a pre-dirty link
- emit deterministic link/unlink and `--dry-run` report data without writing
  config, ignore, ledger, or lock state

## Guardrails

- no Cargo config, ignore, ledger, or lockfile writes
- no `cargo metadata` or `cargo tree` verification after a planned mutation
- no CLI mutation dispatch yet
- no Git restore or destructive cleanup command
- no global Cargo config

## Acceptance

- [x] flat and nested workspaces produce one full-closure repo-root plan
- [x] exact declared URLs remain the patch-table keys
- [x] workspace-less library crates map to canonical package paths
- [x] config transforms preserve foreign content and are idempotent
- [x] tracked config, foreign collisions, malformed markers, path deps,
      no-match, and dirty locks are actionable non-mutating outcomes
- [x] dry-run exposes exact config, ignore, ledger, and affected-lock deltas
- [x] unlink planning removes only Effigy-owned content

## Validation

- focused Cargo plan/config fixture tests
- focused tracked-config and dirty-lock safety tests
- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `effigy qa:ci:fast`
- `git diff --check`

## Stop Conditions

Stop and replan if exact source identity cannot select the full closure, one
repo-root config cannot govern the supported nested workspace fixture, or a
safe transform requires normalizing foreign TOML.

## Evidence

- [`../../../logs/2026-08/05-165735-cargo-full-closure-planning.md`](../../../logs/2026-08/05-165735-cargo-full-closure-planning.md)

## Next Task

Execute ready batch card `1055`.
