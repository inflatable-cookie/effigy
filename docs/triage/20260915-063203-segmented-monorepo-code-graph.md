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
- Effigy already has explicit monorepo catalog membership, but task catalogs
  are not necessarily the same boundary as source ownership.
- Acowtancy is a 35 GB checkout with independent applications and packages.
  `apps/farmyard` is roughly 23 GB and `apps/bovine-desktop` roughly 6.3 GB;
  focused work in another application currently pays the root graph walk.
- Operator-confirmed requirement: selecting one segment must not index or
  freshness-scan the whole monorepo. The existing cold root graph times out in
  Acowtancy.

## Tentative architecture

Separate two concerns:

- a **segment** is the named indexing and query scope;
- a **store** is the physical SQLite database and lock boundary.

Every segment is independently lazy whether it uses the default shared store
or opts into another store. Querying one segment must refresh only that
segment's declared roots; sharing a database must never imply indexing every
segment in that database.

- Add repository-owned code-graph configuration, separate from
  `[docs_policy.graph]`, with explicit named segments and repository-relative
  roots/globs.
- Give segments a default shared store so closely related applications can
  reuse indexed shared packages and retain cross-segment relations.
- Let a segment opt into a named store. A distinct store produces a separate
  database, refresh lock, corruption boundary, and disposal boundary. Store
  names are symbolic identifiers; manifests do not choose arbitrary database
  paths.
- Keep freshness and index-run identity per segment even when several segments
  share one store. Do not build a root-wide graph as a prerequisite.
- Walk the segment's declared roots directly rather than walking the repository
  and filtering afterward.
- Shared packages may be declared in more than one segment. A shared store may
  reuse their records; separate stores accept duplicate local indexing in
  exchange for isolation.
- Apply segment scope before FTS ranking, symbol selection, and traversal.
- Keep repository-relative paths in payloads. Within a segment, edges resolve
  across all of its declared roots. A shared store may retain known
  cross-segment edges without admitting sibling results into the query.
  References into another physical store remain explicit boundary evidence and
  must not trigger that store's index implicitly.
- Include segment configuration in freshness identity. A config change must
  invalidate only the affected segment without pretending its old view is
  current.
- Index/query all segments only through an explicit fan-out operation. It is
  allowed to be proportionally expensive and must report per-segment progress
  and failure rather than becoming an implicit prerequisite.
- Return selected segment and available/derived segment evidence in versioned
  JSON. Existing single-repository behavior remains compatible when no segment
  configuration exists.

Illustrative grammar; the segment/store distinction is operator-confirmed but
field spelling is not:

```toml
[graph]
mode = "segmented"

[graph.segments.farmyard]
roots = ["apps/farmyard", "packages/cattle-grid"]

[graph.segments.dairy]
roots = ["apps/dairy", "packages/cattle-grid"]

[graph.segments.cream]
roots = ["apps/cream", "packages/cattle-grid"]

[graph.segments.bovine-desktop]
roots = ["apps/bovine-desktop"]
store = "bovine-desktop"
```

Farmyard, Dairy, and Cream omit `store` and use the default shared database;
Bovine Desktop uses its own database. All four still refresh independently.

Illustrative command only; not operator-confirmed:

```sh
effigy graph explore --segment api "trace request authorization"
```

## Product decisions needed

1. Root ambiguity: should a root-level query with multiple segments fail and
   require `--segment`, use a configured default, or retain whole-repo behavior?
2. CWD inference: should a query from inside exactly one segment select it
   automatically, with explicit flags overriding inference?
3. Shared code: should overlapping roots in the same store reuse one file
   record with many-to-many membership, while isolated stores duplicate it?
   Should traversal stop at the selected segment boundary by default?
4. Whole-repo escape hatch: use an explicit `--all-segments`, a reserved
   segment name, or the absence of `--segment`?
5. Source of truth: keep graph segments explicit, derive them from declared
   catalog members, or allow an opt-in catalog shorthand while retaining graph
   ownership?
6. Documentation context: should `[docs_policy.graph].roots` receive a reserved
   independently lazy docs segment/store so `docs context` never forces a
   monorepo-wide code index?

## Constraints

- No separate daemon, remote index, or model-generated partitioning.
- Do not make task catalogs silently redefine code ownership.
- Preserve exact repository-relative provenance and current JSON compatibility
  for unsegmented repositories.
- Segment filters must apply before ranking/traversal, not merely trim rendered
  output.
- A single-segment command must not walk, fingerprint, lock, open, or require an
  index for any sibling segment.
- Shared files, cross-segment and cross-store edges, stale membership, partial
  shared-store population, and unknown segment/store diagnostics need
  adversarial proof.
- Documentation graph semantics remain owned by `[docs_policy.graph]`; code
  segmentation must not silently reinterpret documentation authority.

## Promotion conditions

- Operator settles root-query default, CWD inference, shared membership,
  segment/store selection, documentation context, and whole-repo escape
  behavior.
- Architecture names the storage/membership/freshness model and its interaction
  with existing path filters and docs context.
- Contract fixes manifest grammar, CLI selection precedence, JSON additions,
  compatibility, and failure modes.
- A bounded task sequence separates manifest/storage/index work from query/CLI
  rollout if one reviewable task would be too broad.

## Next check

Review independent lazy segment indexing, opt-in physical stores, root/CWD
selection, and documentation-context behavior with the operator. If confirmed,
promote current architecture and contract changes before compiling ready g10
tasks.
