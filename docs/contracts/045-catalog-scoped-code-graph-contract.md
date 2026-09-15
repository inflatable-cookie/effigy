# 045 Catalog-Scoped Code Graph Contract

Status: active
Owner: Product architecture and platform maintainers
Architecture: [`027`](../architecture/027-catalog-scoped-code-graph.md)
Roadmap: [`g10.006`](../roadmaps/g10/006-catalog-scoped-code-graph.md)

## Purpose

Define manifest grammar, selection, storage, refresh, query, compatibility, and
failure rules for catalog-scoped code graphs in monorepos.

## Manifest Grammar

A composed catalog manifest may declare:

```toml
[catalog]
alias = "bovine-desktop"

[catalog.graph]
segmented = true
independent = true
```

Both fields are optional booleans and default to `false`.

- `segmented = false` folds the catalog tree into its parent graph corpus.
- `segmented = true` creates an independently lazy catalog scope and prunes that
  catalog root from its parent's scan.
- `independent = true` gives that segmented scope its own physical database and
  refresh lock.
- `independent = true` with `segmented = false` is invalid.

The root continues to declare membership using existing grammar:

```toml
[catalog.members]
farmyard = "apps/farmyard"
bovine_desktop = "apps/bovine-desktop"
```

The root must not repeat graph segment names, catalog roots, or storage
assignments. `[catalog.graph]` does not create membership. Member manifests do
not recursively contribute their own members to a parent, matching contract
`037`.

## Scope Identity And Eligibility

A scope is identified by workspace root, effective catalog alias, and canonical
catalog root. Aliases must remain unique under the existing effective-membership
contract.

V1 segmentation accepts effective member roots that canonically resolve beneath
the workspace root. A symlink or member path that escapes that root is rejected
before index access. Folded external catalogs retain existing catalog routing but
do not become an implicitly scanned external graph corpus.

## Selection Grammar

All `effigy graph` subcommands accept the common selectors:

```text
--catalog <alias>
--all-catalogs
```

Rules:

1. `--catalog` selects exactly one effective catalog by alias.
2. Without `--catalog`, invocation cwd inside a segmented member selects the
   deepest matching effective catalog.
3. Otherwise the root graph scope is selected.
4. `--repo` resolves the workspace before these rules and never changes the
   meaning of a catalog alias.
5. `--all-catalogs` selects root plus every segmented effective catalog as an
   explicit fan-out and conflicts with `--catalog`.
6. Existing `--path` arguments are relative to and contained by the selected
   scope. They never cause sibling indexing or cross-scope results.

An explicit unknown, ambiguous, folded-only, or escaping catalog selection fails
before refresh with a non-zero exit and a diagnostic naming the catalog and
available segmented aliases.

## Scan And Refresh

- Root scans start at the workspace root and prune every segmented effective
  descendant before descent.
- Catalog scans start at that catalog root and prune any segmented descendants
  present in its independently resolved effective membership.
- A single-scope operation must not walk, fingerprint, lock, open, freshness-
  check, or require an index for a sibling scope.
- Shared-database freshness, extractor versions, file state, and index-run
  records are keyed by catalog scope.
- Configuration identity includes alias, canonical root, segmentation,
  independence, prune set, extractor versions, and relevant docs-profile state.
- A scope configuration change invalidates only that scope.
- Explicit fan-out processes scopes separately and returns per-scope outcome;
  one failure does not masquerade as complete success.

Query commands keep the existing bounded time budget. `graph index` and
`graph watch` retain their explicit long-running posture, scoped to the selected
catalog unless fan-out is explicitly requested where supported.

## Storage

The root shared database remains:

```text
<workspace>/.effigy/graph/graph.db
```

Independent catalog storage is:

```text
<workspace>/.effigy/graph/catalogs/<encoded-alias>/graph.db
```

The matching lock is colocated in the same catalog graph directory. Alias
encoding must be deterministic, collision-checked, and unable to escape the
root-owned graph directory. User-configured database paths and named shared-store
pools are not supported.

Every shared-store entity that can affect query, freshness, deletion, or
relationship traversal carries scope identity. Deleting or reindexing one scope
must not delete sibling records.

## Query And Relationship Rules

- Scope predicates apply before FTS ranking, result limits, symbol selection,
  traversal, and context/explore packet assembly.
- A shared store may use already-current sibling records to resolve an external
  relationship, but default results stay inside the selected scope.
- Missing or stale sibling data remains unresolved and never triggers refresh.
- An independent store never opens another catalog database implicitly.
- `affected`, `impact`, `callers`, `callees`, `node`, `files`, `search`,
  `context`, and `explore` all obey the same selected scope.
- Watch events and status/index reports name their selected catalog and storage
  posture.

## Documentation Context

`effigy docs context` and `docs context sources` remain governed by
`[docs_policy.graph]`, contract `041`, and their existing command grammar. Their
repository documentation corpus is logically distinct from catalog code scopes.
Refreshing documentation context must not refresh source catalogs; refreshing a
catalog must not narrow or redefine configured documentation roots.

## JSON And Text Output

Existing graph payload schemas remain at version 1 with additive fields:

- `catalog.alias`;
- `catalog.root`;
- `catalog.selection` (`explicit`, `cwd`, or `root`);
- `catalog.segmented`;
- `catalog.independent`;
- fan-out results carry one complete outcome per catalog.

Single-catalog repositories retain their existing fields and values. Human
output names a non-root catalog or fan-out posture without adding noise to the
compatible root-only case.

## Required Proof

1. A root fixture with two segmented catalogs proves selecting one never opens,
   walks, fingerprints, indexes, or reports files from the sibling.
2. A root query prunes segmented catalogs while retaining loose root files and
   folded member files.
3. Cwd inference and `--catalog` choose the same scope; explicit selection wins.
4. `--all-catalogs` is the only operation that refreshes multiple scopes and
   reports each outcome.
5. Shared storage keeps freshness/deletion/query state isolated per catalog.
6. `independent = true` creates and locks only the deterministic catalog
   database; a sibling store is untouched.
7. Unknown aliases, duplicate selectors, invalid flag combinations, escaping
   members, and `independent` without `segmented` fail before scan.
8. Query ranking and traversal cannot leak sibling results from a shared store.
9. External references do not trigger sibling refresh and remain bounded across
   independent stores.
10. Documentation context retains its configured corpus without refreshing code
    catalogs.
11. Existing unsegmented graph, path-filter, timeout, watch, text, and JSON
    behavior remains unchanged.

## Change Triggers

Revisit this contract when changing catalog graph grammar, effective membership,
scope selection, graph paths, database schema, freshness identity, query
predicates, fan-out behavior, documentation-corpus isolation, or graph JSON
evidence.
