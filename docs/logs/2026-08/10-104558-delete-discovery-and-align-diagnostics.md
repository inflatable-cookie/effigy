# Delete Discovery And Align Diagnostics

Status: complete
Created: 2026-08-10
Roadmap: g08.028
Batch: card-1074-delete-discovery-and-align-diagnostics

## Summary

- Deleted the recursive catalog walker, skip policy, symlink traversal,
  discovery cache state, cache helpers, and their obsolete tests.
- Renamed the remaining routing module and APIs around effective membership.
- Removed `catalog.discovery` from the manifest and doctor schemas.
- Removed the `effigy catalog` command, cache subcommands, help topic, command
  registry entry, and built-in task inventory entry.
- Aligned runner, doctor, tasks, completion, help, errors, and docs on
  declared/effective catalog terminology.
- Kept generic catalog JSON structures stable.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `MAINT`, `OPERATE`
- Movement: baseline `explicit routing coexists with unreachable ambient
  discovery and cache surfaces` -> current `explicit membership is the only
  runtime and public command model`
- Remaining gap: card `1075` must complete consumer-shape proof, full QA, and
  strict-lane closeout.

## Deletion And Exact-Token Evidence

- Nine obsolete source and test files were deleted in the current lane delta,
  including `discovery.rs`, the catalog command runner, its help topic, and the
  discovery/cache test modules.
- Product and current-guide search is empty for `discover_catalogs`,
  `discover_manifest_paths`, `CatalogDiscovery`, `catalog_discovery`,
  `Command::Catalog`, `HelpTopic::Catalog`, `CatalogArgs`, and cache-stamp or
  empty-subtree terminology.
- The only live-tree `catalog.discovery` occurrences are strict rejection
  fixtures and doctor migration remediation. No compatibility alias remains.
- `effigy catalog cache clear` fails with direct root-membership migration
  guidance; general help and task inventory omit the removed surface.

## Validation Performed

- `cargo test -p effigy-routing`
  - result: pass, 6 tests
- `cargo test -p effigy-manifest`
  - result: pass
- `cargo test -p effigy-doctor`
  - result: pass, 58 tests
- `cargo test -p effigy-cli`
  - result: pass, 10 tests
- `cargo test -p effigy --lib` with three live-runtime gateway probes filtered
  - result: pass, 1,353 tests
- `cargo test --test cli_output_tests`
  - result: pass, 237 tests; 1 ignored
- `effigy qa:json`
  - result: pass
- `effigy qa:docs`
  - result: pass after redirecting two historical roadmap source links to the
    renamed membership module
- focused Clippy on routing, manifest, doctor, CLI, core, builtin, and root
  crates
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `git diff --check`
  - result: pass
- refreshed graph plus `effigy graph affected --stdin --json`
  - result: broad shared routing and public-surface impact; root library,
    CLI-output, JSON, and docs suites cover the selected seams

## Boundaries

No selector precedence, generic catalog JSON schema, workflow, or release
surface changed. Ordinary and legacy mounts still do not imply membership.

## Next Task

Execute ready card
[`1075`](../../roadmaps/g08/batch-cards/1075-prove-migration-and-close-explicit-membership-lane.md).
