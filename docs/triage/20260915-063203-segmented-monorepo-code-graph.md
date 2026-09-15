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

## Tentative architecture

Prefer one physical graph database with named logical segment membership over
one database per subproject.

- Add repository-owned code-graph configuration, separate from
  `[docs_policy.graph]`, with explicit named segments and repository-relative
  roots/globs.
- Index the union once. Store file-to-segment membership as a many-to-many
  relation so shared packages can belong to more than one segment.
- Apply segment scope before FTS ranking, symbol selection, and traversal.
- Keep global node identity and cross-segment edges in storage. Default query
  results stay inside the selected segment; an explicit wider mode may expose
  boundary crossings.
- Include segment configuration in freshness identity. A config change must
  reclassify membership deterministically without pretending the old view is
  current.
- Return selected segment and available/derived segment evidence in versioned
  JSON. Existing single-repository behavior remains compatible when no segment
  configuration exists.

Illustrative grammar only; not operator-confirmed:

```toml
[graph]
mode = "segmented"

[graph.segments.api]
roots = ["apps/api", "packages/domain"]

[graph.segments.web]
roots = ["apps/web", "packages/ui", "packages/domain"]
```

Illustrative command only; not operator-confirmed:

```sh
effigy graph explore --segment api "trace request authorization"
```

## Product decisions needed

1. Root ambiguity: should a root-level query with multiple segments fail and
   require `--segment`, use a configured default, or retain whole-repo behavior?
2. CWD inference: should a query from inside exactly one segment select it
   automatically, with explicit flags overriding inference?
3. Shared code: should overlapping segment roots be first-class many-to-many
   membership, and should traversal stop at the selected boundary by default?
4. Whole-repo escape hatch: use an explicit `--all-segments`, a reserved
   segment name, or the absence of `--segment`?
5. Source of truth: keep graph segments explicit, derive them from declared
   catalog members, or allow an opt-in catalog shorthand while retaining graph
   ownership?

## Constraints

- No separate daemon, remote index, or model-generated partitioning.
- Do not make task catalogs silently redefine code ownership.
- Preserve exact repository-relative provenance and current JSON compatibility
  for unsegmented repositories.
- Segment filters must apply before ranking/traversal, not merely trim rendered
  output.
- Shared files, cross-segment edges, stale membership, and unknown segment
  diagnostics need adversarial proof.
- Documentation graph semantics remain owned by `[docs_policy.graph]`; code
  segmentation must not silently reinterpret documentation authority.

## Promotion conditions

- Operator settles root-query default, CWD inference, shared membership, and
  whole-repo escape behavior.
- Architecture names the storage/membership/freshness model and its interaction
  with existing path filters and docs context.
- Contract fixes manifest grammar, CLI selection precedence, JSON additions,
  compatibility, and failure modes.
- A bounded task sequence separates manifest/storage/index work from query/CLI
  rollout if one reviewable task would be too broad.

## Next check

Review the recommended logical-segment model with the operator. If confirmed,
promote current architecture and contract changes before compiling ready g10
tasks.
