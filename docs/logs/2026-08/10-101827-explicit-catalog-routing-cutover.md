# Explicit Catalog Routing Cutover

Status: complete
Created: 2026-08-10
Roadmap: g08.028
Batch: card-1073-explicit-catalog-routing-cutover

## Summary

- Replaced runtime descendant discovery with one routing-owned explicit
  membership normalizer.
- Collected root members, named mount references, and inline catalog mounts
  from the complete composed manifest across every system and workspace.
- Canonicalized and sorted member paths, loaded each physical catalog once,
  and retained every convergent declaration origin.
- Kept ordinary structured mounts, legacy string mounts, and undeclared nested
  manifests out of routing.
- Migrated runner and JSON contract fixtures to declared membership.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `MAINT`, `OPERATE`
- Movement: baseline `runtime membership depends on recursive filesystem and
  mount discovery` -> current `runtime membership is the deterministic union
  of root-owned declarations`
- Remaining gap: card `1074` must delete the unreachable discovery/cache code
  and align the public diagnostic and CLI vocabulary.

## Behavior Evidence

- `effigy tasks` reports one self-host catalog from the current root manifest.
- `effigy test --plan` reports one self-host target.
- Routing fixtures cover declared descendants, siblings, symlink aliases,
  named system mounts, inline system mounts, inline workspace mounts, ordinary
  mounts, undeclared invalid sentinels, and selection-independent membership.
- Built-in test-plan coverage includes both explicit mount shapes and proves
  ordinary and undeclared manifests do not fan out.

## Validation Performed

- `cargo test -p effigy-routing`
  - result: pass, 15 tests
- focused runner catalog, task-plan, completion, tasks, doctor, status,
  preflight, and JSON contract tests
  - result: pass
- `cargo test -p effigy --lib` with three live-runtime gateway probes filtered
  - result: pass, 1,353 tests
- three exact gateway tests with container binaries absent from `PATH`
  - result: pass
- `cargo clippy -p effigy-routing -p effigy-manifest -p effigy-containers -p effigy-doctor -p effigy --all-targets -- -D warnings`
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `effigy graph affected --stdin --json`
  - result: broad shared-routing impact; full root library regression selected
- `effigy qa:docs`
  - result: pass
- `git diff --check`
  - result: pass

## Boundaries

No selector precedence, unique-task ownership, workflow, release, or generic
catalog JSON shape changed. Discovery implementation and cache/CLI deletion
remain bounded to card `1074`.

## Next Task

Execute ready card
[`1074`](../../roadmaps/g08/batch-cards/1074-delete-discovery-and-align-diagnostics.md).
