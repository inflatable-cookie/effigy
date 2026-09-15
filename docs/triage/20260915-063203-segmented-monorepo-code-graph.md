# Segmented monorepo code graph

Status: open
Created: 2026-09-15
Owner: product architecture and code graph
Source: operator report in Effigy Chatterbox

## Issue

Effigy's code graph treats the selected repository as one global corpus. In a
large monorepo, unrelated subprojects dominate indexing, lexical ranking,
traversal, and result payloads. Per-call `--path` filters exist only on
`context` and `explore`; they are repetitive, unnamed, and do not create a
durable project boundary.

## Why it matters

- Cold refresh cost grows with every subproject even when a query concerns one.
- Common names from unrelated applications dilute owner ranking and context.
- Agents must know and repeat path prefixes instead of selecting a stable
  project identity.
- A whole-repository result can look authoritative while mixing independent
  products.

## Current evidence

- `GraphPaths::for_repo` owns one `.effigy/graph/graph.db` and refresh lock.
- `scan_repo_files` walks the complete repository root, subject only to ignore
  rules and fixed generated/dependency directory skips.
- `GraphStore` records repository-relative paths without segment membership.
- `context` and `explore` accept transient path-prefix filters after loading
  the global file/symbol/edge sets. Other graph queries remain global.
- Effigy already has explicit, root-owned monorepo catalog membership and a
  stable alias on each member manifest. That is the operator-confirmed graph
  partition boundary; a second segment namespace would duplicate topology.
- Acowtancy is a 35 GB checkout with independent applications and packages.
  `apps/farmyard` is roughly 23 GB and `apps/bovine-desktop` roughly 6.3 GB;
  focused work in another application currently pays the root graph walk.
- Operator-confirmed requirement: selecting one segment must not index or
  freshness-scan the whole monorepo. The existing cold root graph times out in
  Acowtancy.
- Operator-confirmed requirement: each member catalog declares its own graph
  indexing posture. The root does not repeat catalog roots, segment names, or
  store assignments in a separate graph map.

## Tentative architecture

Use the effective catalog set as the only graph topology:

- A **catalog scope** is the existing catalog alias and catalog-root pair. Do
  not add graph segment names or repeat roots.
- Each member's own composed `effigy.toml` may opt that catalog into independent
  lazy indexing. The root loads this posture while resolving its explicit
  effective membership.
- The root manifest still declares `[catalog.members]`; it needs no additional
  graph topology. A root with no graph posture retains today's single-corpus
  behavior for compatibility.
- An independently indexed member is pruned from its parent's scan. Querying
  the root must therefore never walk, fingerprint, or index that member.
- Querying from within a member uses existing nearest-catalog resolution.
  Explicit selection uses the existing catalog alias vocabulary, such as
  `--catalog bovine-desktop`, rather than a new `--segment` namespace.
- Every independent catalog scope is lazy. Sharing a database must never imply
  refreshing every catalog in that database.
- An independent catalog uses the root-owned shared database by default, with
  catalog-scoped records, freshness, index-run identity, and query predicates.
- A catalog may additionally opt into its own physical database and lock. The
  path is deterministic and root-owned, such as
  `.effigy/graph/catalogs/<alias>/graph.db`; manifests cannot choose arbitrary
  paths or name shared stores.
- Catalogs in the shared database may resolve relationships to already indexed
  catalogs, but a relationship must not trigger sibling indexing. A separate
  database surfaces cross-catalog references as unresolved boundary evidence.
- Apply catalog scope before FTS ranking, symbol selection, and traversal.
  Keep repository-relative paths in payloads.
- Include catalog root, alias, and graph posture in freshness identity. A
  posture change invalidates only the affected catalog view.
- Index/query every catalog only through an explicit fan-out operation. It is
  never an implicit prerequisite and reports progress and failure per catalog.
- Return selected catalog and storage posture in versioned JSON. Existing
  single-catalog JSON remains compatible.

Illustrative member grammar; the behavior is operator-confirmed, while exact
field spelling remains a contract choice:

```toml
# apps/bovine-desktop/effigy.toml
[catalog]
alias = "bovine-desktop"

[catalog.graph]
segmented = true
separate_db = true
```

Farmyard, Dairy, and Cream can set only `segmented = true`: each remains an
independently lazy catalog scope in the shared physical database. Bovine
Desktop also sets `separate_db = true` for an isolated database. The root only
contains its existing membership map:

```toml
[catalog.members]
farmyard = "apps/farmyard"
dairy = "apps/dairy"
cream = "apps/cream"
bovine_desktop = "apps/bovine-desktop"
```

## Product decisions needed

1. Field spelling: use `[catalog.graph]` as the catalog-local indexing posture,
   or a top-level `[graph]` table in every catalog manifest?
2. Root query: after independent members are pruned, should a root-level query
   search only the root corpus, or also query already-current shared-store
   members without refreshing them?
3. Explicit fan-out: should the escape hatch be `--all-catalogs`, matching the
   existing identity model?
4. Nested membership: the current effective-membership contract does not
   recursively expand child members. Confirm graph scope follows that same
   boundary when a child is independently resolved as a root.
5. Documentation context: should `[docs_policy.graph].roots` remain an
   independent documentation corpus so `docs context` cannot force code-graph
   catalog indexing?

## Constraints

- No separate daemon, remote index, or model-generated partitioning.
- Only explicitly declared effective catalogs can become graph scopes. A
  nested manifest that is not in effective membership remains ordinary source.
- Preserve exact repository-relative provenance and current JSON compatibility
  for unsegmented repositories.
- Catalog scope must apply before ranking/traversal, not merely trim rendered
  output.
- A single-catalog command must not walk, fingerprint, lock, open, or require an
  index for any sibling catalog.
- Cross-catalog and cross-store edges, stale membership, partial
  shared-store population, and unknown catalog/storage diagnostics need
  adversarial proof.
- Documentation graph semantics remain owned by `[docs_policy.graph]`; code
  segmentation must not silently reinterpret documentation authority.

## Promotion conditions

- Operator settles field spelling, root-query behavior, explicit fan-out,
  nested membership, and documentation-context behavior.
- Architecture names the storage/membership/freshness model and its interaction
  with existing path filters and docs context.
- Contract fixes manifest grammar, CLI selection precedence, JSON additions,
  compatibility, and failure modes.
- A bounded task sequence separates manifest/storage/index work from query/CLI
  rollout if one reviewable task would be too broad.

## Next check

Review the five remaining product decisions. If confirmed, promote architecture
and contract changes before compiling ready g10 tasks.
