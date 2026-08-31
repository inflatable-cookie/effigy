# Effigy Architecture Overview

Status: active
Updated: 2026-08-29

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

## Vision Alignment

- Primary tags: `MAINT`, `ROUTE`, `OPERATE`
- Target envelope: architecture framing remains clear enough that routing and operational behavior can evolve without boundary drift.
- Vision target delta: baseline architecture docs now explicitly map design intent to vision tags instead of relying on implied interpretation.

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

- [010-package-map.md](./010-package-map.md) is the live crate and module
  ownership map
- [020-container-infrastructure-design.md](./020-container-infrastructure-design.md)
  is longer-form container design background, not the live runtime ownership map
- [021-production-deployment-export-architecture.md](./021-production-deployment-export-architecture.md)
  is the deploy/export architecture anchor
- [023-local-dependency-linking-architecture.md](./023-local-dependency-linking-architecture.md)
  is the active Cargo/Bun machine-local dependency-linking boundary
- [024-repository-defined-documentation-graph.md](./024-repository-defined-documentation-graph.md)
  is the active generic documentation-graph and repository-profile boundary
- [025-external-skill-task-execution.md](./025-external-skill-task-execution.md)
  owns explicit installed-skill source versus consumer-target execution
- [`contract/037`](../contracts/037-explicit-catalog-membership-contract.md)
  defines catalog membership grammar, normalization, routing stability, and
  the ambient-discovery removal boundary
- [`contract/042`](../contracts/042-external-skill-task-runner-contract.md)
  defines isolated skill loading and source/target path semantics

For runtime/container behavior rules, prefer the active contracts:

- `docs/contracts/005-container-runtime-contract.md`
- `docs/contracts/009-execution-surface-convergence.md`
