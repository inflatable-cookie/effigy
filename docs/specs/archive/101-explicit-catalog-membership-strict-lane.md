# 101 - Explicit Catalog Membership Strict Lane

Roadmap: [`g08.028`](../roadmaps/g08/028-explicit-catalog-membership.md)

Durable authority:

- [`architecture/000`](../architecture/000-overview.md)
- [`architecture/010`](../architecture/010-package-map.md)
- [`contract/037`](../contracts/037-explicit-catalog-membership-contract.md)
- [`working rules/001`](../contracts/001-working-rules.md)

Status: Complete
Owner: Platform
Created: 2026-08-10

## Purpose

Replace recursive ambient catalog discovery with explicit root-owned catalog
membership.

Catalog membership is routing policy. The presence of a nested
`effigy.toml`, a symlink, or a mounted repository must not silently add tasks
to the parent catalog.

## Lane Posture

Posture: `strict-ready`

Current ready card:

- [`1072`](../roadmaps/g08/batch-cards/1072-add-explicit-member-and-typed-mount-schema.md)

Implementation must follow the current ready card.

## Decision

The effective catalog set is the deterministic union of:

1. the resolved workspace-root catalog
2. named members declared by the composed root manifest
3. structured system or workspace mounts with `catalog = true`

Named catalog members may be referenced by structured mounts so a source path
does not need to be repeated.

Plain mount strings and structured source mounts without `catalog = true`
never imply catalog membership.

Effigy must not recursively walk descendants to build the runtime catalog set.
Ancestor walk-up used to locate the invocation's repo or workspace root remains
separate and in scope.

## Manifest Grammar

### Named members

```toml
[catalog]
alias = "root"

[catalog.members]
web = "apps/web"
api = "services/api"
underlay = "../underlay"
```

Rules:

- member keys are root-local handles used by mount references
- routing aliases still come from each member's own manifest
- values name catalog directories; Effigy appends `effigy.toml`
- values resolve relative to the composed root manifest directory
- relative paths may leave the root, including sibling repositories
- absolute paths, globs, and direct manifest-file paths are rejected
- each resolved directory must exist and contain `effigy.toml`
- child `catalog.members` declarations are not expanded while that child is a
  member of another root
- the same child may use its own members when independently resolved as a root

The composed root includes committed fragments and an enabled local overlay.
Members contributed by those layers are explicit inputs. Existing local-overlay
disable controls continue to define whether machine-local declarations enter
the effective manifest.

### Structured system and workspace mounts

Both `systems.<name>.mounts` and
`systems.<name>.workspaces.<name>.mounts` accept existing strings plus a new
structured form:

```toml
[systems.dev]
mounts = [
  { member = "underlay", target = "/workspace/underlay" },
  { source = "../tooling", target = "/workspace/tooling", catalog = true },
  { source = "./data", target = "/data" },
]
```

Structured fields:

- exactly one of `member` or `source`
- optional `target`; omission preserves the current basename-derived target
- optional `options` string or string array, normalized to the existing mount
  option representation
- optional `catalog` boolean only on a `source` mount; default `false`

Semantics:

- `member = "underlay"` resolves the source through
  `catalog.members.underlay`
- member references are always catalog-backed and reject a redundant
  `catalog` field
- `source = "...", catalog = true` declares an anonymous explicit member
- `source = "..."` without the flag is runtime topology only
- legacy string mounts remain runtime topology only, even when their source
  contains `effigy.toml`
- inline source paths retain the existing system-mount path-resolution rules
- a catalog-backed mount must resolve to a directory containing `effigy.toml`

## Normalized Model

All declaration forms feed one routing-owned model before catalogs load:

```text
CatalogMember {
  handle: optional string,
  catalog_root: resolved canonical path,
  manifest_path: catalog_root/effigy.toml,
  origins: one or more declaration origins,
}
```

Declaration origins distinguish:

- root catalog
- named root member
- system mount member reference
- workspace mount member reference
- inline catalog system mount
- inline catalog workspace mount

Origins are diagnostic evidence, not competing resolution modes.

Normalization rules:

- collect declarations from every configured system and workspace, independent
  of the selected runtime system
- resolve and canonicalize existing member directories
- deduplicate identical canonical paths before manifest loading
- preserve every origin when declarations converge on one path
- load each catalog manifest once
- retain the existing duplicate routing-alias error across distinct paths
- sort the normalized set deterministically before catalog loading and output

Task availability must not vary with the active system. Systems may control
whether a catalog is mounted into a runtime, but not whether the declared
catalog exists in the task surface.

## Error Contract

Configuration fails before selector routing when:

- a named member handle is empty or duplicated by composition
- a member reference is unknown
- a structured mount declares neither or both of `member` and `source`
- `catalog` appears on a member reference
- a member path is missing, not a directory, or lacks `effigy.toml`
- a named member uses an absolute path, glob, or direct manifest path
- two distinct normalized paths load the same catalog alias

Diagnostics identify the root manifest, declaration origin, member handle when
present, raw source, and resolved path.

`effigy doctor`, `effigy tasks`, selector failures, and JSON output must consume
the same normalized membership evidence. They must not reimplement discovery.

## Routing And Runtime Effects

- selector precedence remains unchanged after the explicit catalog set loads
- unique unprefixed task ownership remains unchanged
- built-in `test` fans out only across explicit members
- demos, task status, execution preflight, and doctor consume the same set
- mounted producer isolation remains mount-driven and independent of catalog
  membership
- `[manifest].root = true` remains a repo/workspace-root boundary; it no longer
  prunes descendant catalog discovery because no descendant walk exists

## Removed Surface

The breaking change removes:

- recursive descendant catalog walking
- automatic membership for mounted repositories
- `[catalog.discovery] enabled`
- `[catalog.discovery] ignore`
- the catalog-discovery cache and empty-subtree cache
- `effigy catalog cache clear`
- the now-empty `effigy catalog` built-in and inventory entry
- discovery-only symlink traversal and skip-directory policy

No ambient-discovery fallback or dual runtime mode is retained. This is a
pre-1.0 configuration break and should land in one declared breaking release.

Stale repo-local discovery cache files are ignored. Effigy does not need to
mutate or clean them during normal command execution.

## Migration

Consumer migration is explicit:

1. list intended child catalogs under `[catalog.members]`
2. convert mounted catalog paths to a named `member` reference or a structured
   source mount with `catalog = true`
3. leave ordinary mounts unmarked
4. remove `[catalog.discovery]`
5. run `effigy doctor`, `effigy tasks`, and `effigy test --plan`

No product-owned migration scanner is required for the first lane. Adding one
would retain the filesystem-walk machinery this change is intended to delete.

## Owner And Seam

- `effigy-manifest` owns the named-member and typed system-mount schema
- `effigy-routing` owns normalization, canonical identity, manifest loading,
  alias validation, sorting, and membership evidence
- `effigy-containers` consumes the typed mount model for runtime rendering
- `effigy-doctor` consumes shared validation/evidence and owns presentation
- `effigy-cli` owns removal of the obsolete cache command and help text
- the root runner remains an adapter over the routing-owned catalog set

No second member resolver may live in runner, doctor, test orchestration, or
container code.

## Promotion State

Promoted:

1. architecture `000` owns the explicit-membership frame
2. architecture `010` owns crate and runner seams
3. contract `037` owns grammar, normalization, errors, routing stability,
   migration, and removals

Remaining:

1. execute cards `1072` through `1075`
2. archive this spec after the durable surfaces and roadmap fully own the lane

## Proposed Batch Shape

1. contract and architecture promotion
2. typed manifest members and structured system mounts
3. routing-owned explicit normalization and consumer migration
4. walker, cache, CLI, and compatibility-surface deletion
5. self-host, nested-repo, sibling-mount, docs, and JSON proof

This sequence is provisional. Roadmap compilation owns final card boundaries.

## Acceptance

- [x] the root manifest explicitly defines every non-root catalog
- [x] both named mount references and inline `catalog = true` mounts work
- [x] plain mounts never affect catalog membership
- [x] membership is stable across active-system selection
- [x] catalogs are canonicalized, deduplicated, sorted, and loaded once
- [x] selector precedence and unique-task routing remain stable
- [x] test planning fans out only across explicit catalogs
- [x] doctor reports shared declaration evidence and precise failures
- [x] recursive discovery, cache state, and the cache CLI are deleted
- [x] current self-host, nested consumer, symlink, and sibling-repo shapes pass
- [x] public docs and changelog identify the configuration break
- [x] no release mutation or workflow edit occurs in the lane

## Stop Conditions

Stop and replan if:

- runtime correctness requires a recursive descendant walk or glob expansion
- catalog membership would depend on the selected system
- child-member recursion is required
- manifest, routing, doctor, and runner need separate member-resolution models
- structured mounts cannot preserve current string-mount runtime behavior
- the work requires a workflow edit or release mutation

## Settled Details

- structured mount `options` accepts a string array only
- stable JSON structures named for catalogs remain unchanged
- evidence and identifiers explicitly named for ambient discovery move to
  membership terminology as part of the declared break

## Next Task

Lane complete. Contract `037` and roadmap `g08.028` own the durable behavior.
