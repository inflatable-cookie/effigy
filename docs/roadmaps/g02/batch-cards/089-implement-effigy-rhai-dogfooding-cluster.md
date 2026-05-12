# 089 Implement Effigy Rhai Dogfooding Cluster

Status: archived
Updated: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/archive/004-rust-native-scripting-strict-lane.md`

## Objective

Dogfood the shipped Rhai script-step surface inside Effigy by migrating a
meaningful cluster of remaining shell-glue tasks to file-backed Rhai scripts,
so the next host-API gaps are discovered in the first-party repo before any
cross-repo Rhai pilot starts.

## In Scope

- identify a meaningful cluster of remaining Effigy shell-glue tasks that are:
  - repo-local automation glue
  - not strongly dependent on shell semantics
  - large enough to test the surface beyond one trivial helper
- migrate those tasks to `rhai = "scripts/..."` file-backed scripts
- keep the scripts in a coherent repo-local Rhai area under `scripts/`
- tighten docs/examples if the dogfooding batch exposes missing guidance
- record the first real host-API gaps discovered by the dogfooding pass

## Out Of Scope

- Keepsake migration
- Jetstream migration
- release-path redesign
- arbitrary shell emulation
- adding broad new Rhai host APIs without clear dogfooding pressure

## Acceptance Criteria

- multiple real Effigy shell-glue tasks migrate to file-backed Rhai
- the migrated tasks still validate through normal Effigy QA
- the batch leaves an honest note about what Rhai still cannot replace cleanly
  in Effigy

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

This card is complete. Use
[`090-decide-post-effigy-rhai-dogfooding-slice.md`](./090-decide-post-effigy-rhai-dogfooding-slice.md)
next.
