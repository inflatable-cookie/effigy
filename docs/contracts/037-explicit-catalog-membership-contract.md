# 037 Explicit Catalog Membership Contract

Status: active
Owner: Platform maintainers
Spec: [`101`](../specs/archive/101-explicit-catalog-membership-strict-lane.md)

## Purpose

Define root-owned catalog membership without recursive descendant discovery.

Catalog membership is routing policy. A nested `effigy.toml`, symlink, or
mounted repository does not join a parent task surface unless the composed
root manifest declares it.

## Terminology

- **root catalog**: the catalog at the resolved workspace root
- **named member**: a catalog directory declared under `[catalog.members]`
- **member handle**: the root-local key used to reference a named member
- **inline member**: a structured system or workspace source mount marked
  `catalog = true`
- **ordinary mount**: a string mount or structured source mount not marked as
  a catalog
- **effective membership**: the normalized catalog set consumed by routing,
  tasks, test planning, execution, demos, status, and doctor
- **declaration origin**: the manifest location and declaration form that added
  evidence for a member

## Root Membership Grammar

```toml
[catalog]
alias = "root"

[catalog.members]
web = "apps/web"
api = "services/api"
underlay = "../underlay"
```

Rules:

- `[catalog.members]` is a map from non-empty local handles to directory paths.
- Handles support mount references. They do not override child routing aliases.
- Each path resolves relative to the composed root manifest directory.
- Relative paths may identify descendants, symlinks, or sibling repositories.
- Absolute paths, globs, and direct `effigy.toml` paths are invalid.
- Each resolved directory must exist and contain `effigy.toml`.
- The composed root manifest may receive members from committed includes and an
  enabled local overlay under normal composition rules.
- Member manifests do not recursively contribute their own members to the
  parent. The same manifest may use its members when independently resolved as
  a root.

The root catalog is always present. A root with no members is a valid
single-catalog project.

## System And Workspace Mount Grammar

`systems.<name>.mounts` and
`systems.<name>.workspaces.<name>.mounts` accept existing strings and
structured entries:

```toml
[systems.dev]
mounts = [
  { member = "underlay", target = "/workspace/underlay" },
  { source = "../tooling", target = "/workspace/tooling", catalog = true },
  { source = "./data", target = "/data", options = ["ro"] },
]
```

A structured entry:

- declares exactly one of `member` and `source`
- may declare `target`; omission retains basename-derived target behavior
- may declare `options` as a string array
- may declare `catalog` only with `source`; default is `false`

Forms:

- `member = "underlay"` reuses the source from
  `catalog.members.underlay`; it is catalog-backed by definition
- `source = "...", catalog = true` declares an anonymous inline member
- `source = "..."` declares runtime topology only
- an existing string mount declares runtime topology only

A member reference rejects `source` and `catalog`. An inline member uses the
existing system-mount source resolution rules and must resolve to a directory
containing `effigy.toml`.

## Effective Membership

Effective membership is the union of:

1. the root catalog
2. every named root member
3. every inline member declared by any configured system or workspace

Collection spans the complete effective manifest. It does not depend on the
selected runtime system or workspace. Selecting a system may change runtime
mounting; it must not change task availability.

Named member references add mount-origin evidence to the referenced member.
They do not create a second catalog identity.

Ordinary mounts never affect membership, even when their source directory
contains `effigy.toml`.

## Normalization

`effigy-routing` owns one normalized member model with:

- optional member handle
- canonical catalog-root path
- `<catalog-root>/effigy.toml` manifest path
- one or more declaration origins

Before manifest loading, routing must:

1. collect all explicit declarations
2. resolve and canonicalize existing directories
3. deduplicate identical canonical paths while retaining all origins
4. sort members deterministically
5. load each manifest once
6. reject one alias owned by multiple distinct canonical paths

Symlink and physical declarations resolving to one canonical directory are one
member. Declaration origin remains visible for diagnostics.

No runner, doctor, test, demo, status, or container surface may implement a
second membership resolver.

## Root Resolution Boundary

Invocation root resolution remains separate from catalog membership.

Effigy may walk ancestors to find the applicable repo or workspace root and
continues to honor `[manifest].root = true` as a root boundary. It must not walk
root descendants to find catalog members.

This preserves cwd-aware invocation without restoring ambient membership.

## Routing Guarantees

After effective membership loads:

- alias, path, cwd-nearest, and shallowest selector precedence is unchanged
- unique unprefixed task ownership is unchanged
- built-in `test` fans out only across effective membership
- task listings, resolution evidence, execution preflight, demos, task status,
  and doctor use the same normalized set
- mounted producer isolation remains mount-driven and independent of catalog
  membership

## Error Contract

Errors must identify declaration origin, handle when present, raw source, and
resolved path when available.

Configuration is invalid when:

- a member handle is empty
- a named path is absolute, globbed, or points directly at a manifest
- a member reference is unknown
- a structured mount declares neither or both of `member` and `source`
- a member reference declares `catalog`
- a member directory is missing, not a directory, or lacks `effigy.toml`
- distinct canonical member paths load the same routing alias

Routing consumers fail before selector selection on invalid membership.
Doctor reports the same shared failure evidence without inventing a parallel
validation model.

## JSON And Diagnostic Stability

- Existing schema IDs and structural fields that say `catalogs` remain stable
  when their shape is unchanged.
- Human text changes from discovered catalogs to declared or effective
  catalogs.
- Finding IDs, evidence fields, or payload values explicitly named for ambient
  discovery must move to membership terminology in the breaking change.
- Removed discovery-cache payloads receive no compatibility alias.
- Any structural JSON change requires the normal schema, example, selection,
  changelog, and dated-log update in the same implementation batch.

## Removed Behavior And Surface

The explicit-membership change removes:

- recursive descendant catalog walks
- automatic catalog membership from mounted repositories
- `[catalog.discovery] enabled`
- `[catalog.discovery] ignore`
- discovery skip-directory and symlink traversal policy
- discovery and empty-subtree caches
- `effigy catalog cache clear`
- the now-empty `effigy catalog` built-in and inventory entry

No ambient fallback, deprecation mode, or dual resolver remains. This is one
documented pre-1.0 breaking configuration change.

Stale discovery-cache files are ignored. Normal commands do not delete them.

## Ownership

- `effigy-manifest`: member grammar, typed mount grammar, composition
- `effigy-routing`: normalization, canonical identity, manifest loading,
  sorting, alias validation, membership evidence
- `effigy-containers`: typed mount rendering and runtime behavior
- `effigy-doctor`: shared evidence consumption and operator presentation
- `effigy-cli`: command grammar and removal of the obsolete cache command
- root runner: adapter over the routing-owned effective set

## Migration Contract

Consumers:

1. declare intended child catalogs under `[catalog.members]`
2. convert mounted catalogs to named member references or inline
   `catalog = true` mounts
3. leave ordinary mounts unmarked
4. remove `[catalog.discovery]`
5. verify with `effigy doctor`, `effigy tasks`, and `effigy test --plan`

No product-owned recursive migration scanner is required.

## Validation

- focused `effigy-manifest` parsing and rejection fixtures for both mount forms
- focused `effigy-routing` normalization, ordering, deduplication, alias, and
  error-evidence fixtures
- selector and built-in test-plan parity across root, nested, symlink, sibling,
  named-mount, inline-mount, and ordinary-mount shapes
- `effigy-containers` string/structured mount rendering parity
- doctor schema and shared-evidence tests
- CLI/help/cache-command removal tests
- JSON examples and contract selection when payload structure changes
- current-repo doctor, tasks, test-plan, docs, and full QA proof

## Drift Triggers

- catalog member grammar changes
- mount reference or inline-member semantics change
- membership starts depending on the selected system
- member canonicalization, ordering, or alias rules change
- a consumer adds its own member resolver
- descendant walking or ambient mount inference returns
- discovery-era CLI, config, cache, diagnostic, or JSON terminology remains

## Next Task

Roadmap [`g08.028`](../roadmaps/g08/028-explicit-catalog-membership.md) is
complete. Await an operator-selected follow-up if this contract needs to
change.
