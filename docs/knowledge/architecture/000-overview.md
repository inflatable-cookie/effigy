# Effigy Architecture Overview


Effigy is a Rust CLI task runner with two responsibility layers:

1. Runner infrastructure:
- CLI parsing and command routing,
- root resolution,
- explicit catalog membership resolution,
- deterministic task selection,
- command execution.

2. Built-in tasks:
- task-specific collect/evaluate/render lifecycle,
- deterministic report output for operational tasks.

## Key design properties

- Catalogs are file-based (`effigy.toml`) so task ownership can live close to the code it operates on.
- The composed root manifest explicitly owns catalog membership. Nested
  manifests and ordinary mounts do not join the parent task surface by
  presence alone.
- Root resolution may walk invocation ancestors. Catalog membership never
  requires a recursive descendant walk.
- Execution is cwd-aware but explicit override friendly via `--repo`.
- Explicit `effigy skill` execution may separate task-definition source from
  consumer target without changing ordinary catalog/root behavior.
- Root detection uses nearest marker semantics across `package.json`, `composer.json`, `Cargo.toml`, and `.git`.
- Unprefixed task resolution is deterministic and fails loudly on ambiguity.
- Task command payloads remain shell commands for incremental adoption.

## Current Authority Surfaces

Use these docs intentionally:

- [010-package-map.md](010-package-map.md) is the live crate and module
  ownership map
- [container runtime contract](../contracts/005-container-runtime-contract.md)
  is longer-form container design background, not the live runtime ownership map
- [021-production-deployment-export-architecture.md](021-production-deployment-export-architecture.md)
  is the deploy/export architecture anchor
- [023-local-dependency-linking-architecture.md](023-local-dependency-linking-architecture.md)
  is the active Cargo/Bun machine-local dependency-linking boundary
- [024-repository-defined-documentation-graph.md](024-repository-defined-documentation-graph.md)
  is the active generic documentation-graph and repository-profile boundary
- [025-external-skill-task-execution.md](025-external-skill-task-execution.md)
  owns explicit installed-skill source versus consumer-target execution
- [026-feature-placement-and-command-surface.md](026-feature-placement-and-command-surface.md)
  defines semantic core ownership, group-first command organization, provider
  placement, catalog-pack constraints, and release/distribution separation
- [027-catalog-scoped-code-graph.md](027-catalog-scoped-code-graph.md) defines
  catalog-derived graph scopes, lazy segmented indexing, and shared versus
  independent storage
- [028-published-and-draft-task-surfaces.md](028-published-and-draft-task-surfaces.md)
  separates maintained published selectors from lifecycle-labelled provisional
  drafts without creating a second execution runtime
- [029-bounded-doctor-and-scan-cache.md](029-bounded-doctor-and-scan-cache.md)
  separates fast structural diagnosis from explicit catalog-scoped deep work
  with one shared inventory, exact incremental cache facts, and deadlines
- [`contract/037`](../contracts/037-explicit-catalog-membership-contract.md)
  defines catalog membership grammar, normalization, routing stability, and
  the ambient-discovery removal boundary
- [`contract/042`](../contracts/042-external-skill-task-runner-contract.md)
  defines isolated skill loading and source/target path semantics
- [`contract/043`](../contracts/043-feature-placement-and-surface-migration-contract.md)
  defines feature-placement gates and alias-stable migration rules
- [`contract/045`](../contracts/045-catalog-scoped-code-graph-contract.md)
  defines catalog graph grammar, selection, storage, refresh, and query
  isolation
- [`contract/046`](../contracts/046-published-and-draft-task-surface-contract.md)
  defines published/draft manifest grammar, discovery, routing, reference
  direction, expiry evidence, and execution compatibility
- [`contract/047`](../contracts/047-bounded-doctor-and-scan-cache-contract.md)
  defines doctor tiers, catalog scope, budgets, cache trust, cancellation, and
  incomplete-report evidence

For runtime/container behavior rules, prefer the active contracts:

- `docs/knowledge/contracts/005-container-runtime-contract.md`
- `docs/knowledge/contracts/009-execution-surface-convergence.md`
