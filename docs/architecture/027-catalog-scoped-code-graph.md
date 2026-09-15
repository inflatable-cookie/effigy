# Catalog-Scoped Code Graph Architecture

Status: active
Updated: 2026-09-15
Contract: [`045`](../contracts/045-catalog-scoped-code-graph-contract.md)

## Purpose

Large monorepos cannot treat every declared product as one mandatory code-graph
corpus. Cold graph queries must reach the selected application without walking,
fingerprinting, or indexing unrelated catalog trees.

Effigy's existing catalog model already owns monorepo topology and stable local
identity. Code-graph partitioning therefore attaches an indexing posture to a
catalog instead of introducing a second segment registry.

## Authority Model

The composed root manifest remains the sole owner of effective catalog
membership. A child manifest participates only when the root declares it under
`[catalog.members]` or through the existing typed catalog-mount forms.

Each member's own composed manifest may declare `[catalog.graph]`. That table
answers how the parent workspace indexes this catalog; it does not add the
catalog to membership, rename it, or grant recursive discovery.

The catalog alias and canonical catalog root form the graph-scope identity.
There are no graph-specific segment names or duplicate root lists. A nested
manifest outside effective membership remains ordinary source.

## Scope Postures

Catalogs have three effective postures:

| Posture | Manifest | Index behavior | Storage |
| --- | --- | --- | --- |
| folded | no graph block, or `segmented = false` | included in the parent corpus | parent store |
| segmented/shared | `segmented = true` | independently lazy; pruned from parent scans | root shared database |
| segmented/independent | `segmented = true`, `independent = true` | independently lazy; pruned from parent scans | catalog database and lock |

`independent` means physical storage isolation. It does not control lazy refresh:
all segmented catalogs refresh independently.

The root manifest needs no graph topology. Its graph scope contains root-owned
files plus folded member trees. Segmented member roots are pruned before descent,
so a root query cannot accidentally pay their scan cost.

## Selection

Graph selection reuses catalog routing concepts:

1. explicit `--catalog <alias>`;
2. nearest in-scope segmented catalog for the invocation cwd;
3. root graph scope.

An explicit `--repo` selects the consumer workspace before catalog selection; it
does not name a catalog. Existing query-local `--path` filters narrow the
selected catalog and cannot widen it.

`--all-catalogs` is the only fan-out. It visits the root scope and each segmented
effective member separately, reports per-catalog progress/failure, and never
turns ordinary queries into whole-workspace refreshes.

## Index And Storage Ownership

The code-graph crate owns a scope descriptor containing the workspace root,
catalog alias, canonical catalog root, parent-prune set, and storage posture.
Walkers start at the selected scope root and prune segmented descendants before
descent. They never walk the workspace and filter afterward.

The shared database stays at `.effigy/graph/graph.db`. Shared records,
freshness state, and index runs carry catalog scope identity. Queries constrain
FTS ranking, symbol selection, and traversal before result limiting.

Independent databases use a deterministic root-owned location:

```text
.effigy/graph/catalogs/<encoded-alias>/graph.db
```

Each has its own refresh lock. Manifests cannot supply database paths or named
store pools. Alias encoding must be deterministic and traversal-safe.

Changing catalog root, alias, segmentation, or independence invalidates the
affected scope. It does not make sibling scopes stale. Removing a segmented
catalog removes its active scope; stale records may be reclaimed without opening
or refreshing another scope.

## Relationships

A shared store may resolve relationships to records from another catalog only
when that catalog is already indexed and current. Relationship discovery never
triggers sibling refresh. Query results remain scoped unless explicit fan-out is
active.

Independent stores retain local records and surface external references as
unresolved boundary evidence. They do not open sibling databases implicitly.

Repository-relative paths remain unchanged for the root and descendant members.
V1 segmented graph scopes must resolve beneath the workspace root; an external
or escaping catalog fails preflight with a useful diagnostic rather than storing
ambiguous `..` provenance.

## Documentation Boundary

`[docs_policy.graph]` remains the sole authority for `effigy docs context`.
Documentation context compiles and refreshes its repository-owned corpus without
forcing code-graph catalog scopes to refresh. Catalog segmentation cannot remove
an explicitly configured documentation root or reinterpret documentation
authority.

## Compatibility

A repository with no segmented catalog retains the current single database,
paths, query defaults, and JSON schemas. Catalog evidence is additive in graph
payloads. Existing `--path`, bounded-query timeout, graph watch, and explicit
index behavior remain intact inside the selected scope.

## Failure Boundary

Fail before scanning when:

- `independent = true` appears without `segmented = true`;
- an explicit catalog is unknown, duplicated, not an effective member, or
  escapes the workspace root;
- a catalog database identity/path cannot be derived safely;
- `--all-catalogs` is combined with a single-catalog selector.

One catalog's lock, corruption, extractor failure, or timeout must not require
opening, repairing, or invalidating any sibling catalog.
