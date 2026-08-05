# g08.020 - Cargo Local Dependency Linking

Status: Complete
Depends on: `g08.019`

## Goal

Implement reversible local Cargo source linking through managed repo-root
patch config while preserving committed manifests and protecting lockfiles.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: one command links the complete matching Cargo closure across
  flat and nested workspaces and proves the resulting source paths.
- Vision target delta: the manual Cargo patch runbook becomes an idempotent,
  verified Effigy operation.

## Scope

- inventory library workspaces through Cargo metadata
- support workspace-less multi-crate library layouts
- inspect every consumer Cargo workspace under the repo root
- distinguish git/tag, path, registry, and unmatched sources
- group matching crates by exact declared git source URL
- plan the full direct/transitive closure
- write/remove managed blocks in one repo-root `.cargo/config.toml`
- use canonical absolute package paths
- preserve hand-managed config and refuse collisions/tracked config
- add/report `.cargo/config.toml` gitignore coverage
- refuse a pre-dirty affected tracked `Cargo.lock`
- verify path resolution after link and committed git resolution after unlink
- re-resolve lockfiles non-destructively on unlink
- report per-crate before/after source evidence in text and JSON

## Non-Goals

- no manifest migration from path to git/tag
- no global `~/.cargo/config.toml` mutation
- no destructive git restore
- no partial-link escape hatch

## Execution Plan

- [x] [`1054`](./batch-cards/1054-plan-cargo-full-closure-and-managed-config.md)
      — produce full-closure Cargo plans and collision-safe managed config,
      ignore, ledger, and lockfile deltas without writes
- [x] [`1055`](./batch-cards/1055-apply-and-verify-cargo-links.md)
      — apply Cargo links, verify the complete local closure, and expose the
      link CLI/JSON path
- [x] [`1056`](./batch-cards/1056-apply-cargo-unlink-and-closeout.md)
      — apply safe unlink, prove committed-source and lock recovery, and close
      the Cargo milestone

## Acceptance Criteria

- [x] library inventory covers workspace and standalone multi-crate fixtures
- [x] all matching direct/transitive crates are patched or the operation fails
- [x] nested consumer workspaces share the repo-root config correctly
- [x] re-link refreshes state without duplicate TOML tables/entries
- [x] unlink removes only Effigy-owned blocks and cleans empty Effigy-created
      files/directories
- [x] tracked config, manual collisions, path deps, no-match, and dirty lockfile
      produce actionable non-mutating outcomes
- [x] dry-run reports the exact config/gitignore delta without writes
- [x] Cargo verification records local paths after link and git sources after
      unlink

## Validation

- focused Cargo inventory/config/state fixture tests
- parser/report JSON fixtures from `g08.019`
- nested-workspace integration fixture
- `cargo tree` before/after proof in temporary repositories
- `effigy qa:ci:fast`

## Next Task

Continue active Bun milestone `g08.021` through ready card `1058`.
