# Bun Committed Dependency Pinning Contract

Status: Implemented and validated
Owner: Platform maintainers
Architecture: [`023`](../architecture/023-local-dependency-linking-architecture.md)
Planning source: [`spec 104`](../specs/archive/104-bun-committed-dependency-pinning.md)
Roadmap: [`g08.031`](../roadmaps/g08/031-bun-committed-dependency-pinning.md)

## Purpose

Define a committed counterpart to save-less Bun links for dependency graphs
that cross repository and `file:` boundaries.

Pinning authors consumer-level Bun `overrides` as reviewable repository state.
It is not a mode of `deps link`: links are ephemeral and machine-local, while
pins are committed and inherited by CI and teammates.

The command, domain, and disposable Soundcheck/Poodle consumer proof are
complete. Roadmap `g08.031` records the closed implementation lane.

## Terminology

- **consumer**: the repository selected by cwd or `--repo`
- **library**: the local checkout named by `LIBRARY_PATH`
- **pin**: a root-consumer Bun override pointing a package at the library
- **closure**: every named package from the library present in the consumer's
  direct or transitive Bun graph
- **canonical-equivalent**: two paths that resolve to the same local package
  root after resolving the `file:` value from the consumer manifest directory

## Command Grammar

```text
effigy deps pin bun <LIBRARY_PATH> [--dry-run]
effigy deps unpin bun <LIBRARY_PATH> [--dry-run]
```

- Standard leading `--repo <PATH>` and `--json` behavior applies.
- Relative library paths resolve from the selected consumer repository, not
  the caller's working directory.
- `--dry-run` plans and reports without writing.
- Cargo pinning is unsupported. `pin cargo` and `unpin cargo` must return an
  explicit unsupported-manager error; they must not fall through to Cargo
  patch behavior.

## Authority and Mutation Boundary

The root consumer `package.json` top-level `overrides` object is the only
desired-state store and the only file pin or unpin may mutate.

Pin and unpin must not:

- write the machine-local dependency-link ledger
- write the Bun registration index
- register packages or create/remove symlinks
- run `bun install`
- change `bun.lock` or `bun.lockb`
- edit the library or an intermediate dependency repository
- choose or change registry versions

The operator runs `bun install` after the manifest edit and reviews the
resulting lockfile separately.

## Package Selection

Pin planning:

1. resolves the consumer and canonical library path
2. inventories named root and workspace packages from the library
3. inspects the consumer dependency tree with read-only `bun pm ls --all`
4. selects every library package present in the direct or transitive graph
5. collapses duplicate resolved copies by package name
6. produces one atomic override plan for the complete matched closure

No match reports a no-write outcome. Pin does not require an earlier link
plan. A direct match and all matched transitive library packages belong to one
operation; partial pinning is a correctness error.

Each value is a relative `file:` specifier from the consumer `package.json`
directory to the canonical local package root:

```json
"overrides": {
  "@inflatable-cookie/poodle-core": "file:../poodle/packages/core",
  "@inflatable-cookie/poodle-svelte": "file:../poodle/packages/svelte/components"
}
```

Absolute override paths are forbidden. A path that escapes the consumer repo
is allowed with a portability warning: CI and teammates need the same relative
checkout topology.

## Pin Rules

- Create a missing top-level `overrides` object without reordering unrelated
  fields.
- Preserve every unrelated existing override.
- Treat an exact canonical-equivalent value as already pinned.
- If any selected package has a conflicting override, refuse the whole
  operation. Never overwrite the conflict or write a partial closure.
- Refuse invalid JSON or a non-object `overrides` value.
- Repeating the same pin is idempotent.
- After apply, verify the written manifest entries. Report dependency
  resolution as pending until the operator runs `bun install`.

## Unpin Rules

Unpin is an explicit manifest edit, not an ownership-ledger rollback.

- Inventory the named library checkout without consulting the consumer Bun
  graph.
- Remove only entries whose package name belongs to that library and whose
  `file:` value canonically resolves to the corresponding local package root.
- Preserve same-name entries that point elsewhere and every unrelated entry.
- Remove the `overrides` object only when the operation removed its final
  entry.
- Report an already-unpinned no-op when no exact entries match.
- Require the library checkout to exist. Path guessing from a missing checkout
  is unsupported.

Unpin does not restore a hidden previous value. Pin never overwrites a
conflicting value, so there is no previous state to recover.

## Manifest Write Safety

- Plan from an exact byte snapshot and compare those bytes immediately before
  apply.
- Refuse a concurrent content change instead of overwriting it.
- Use one atomic replacement for the consumer manifest.
- Preserve unrelated keys, object order, indentation, and final-newline
  posture.
- Add or remove only the planned override entries.
- Leave the original manifest bytes intact on write failure.
- Prove both Bun lockfile forms remain byte-for-byte unchanged.

A dirty Git worktree is allowed. The command must preserve intentional
unrelated edits in the same manifest.

## Interaction With Machine-Local Links

Contract [`034`](./034-local-dependency-linking-contract.md) remains authority
for link, unlink, status, and doctor behavior.

- Pin refuses while an Effigy-managed Bun link for the same library or package
  is active. The next action is to unlink first.
- Link recognizes a matching committed override and produces no link mutation.
  The next action is to use the pin plus `bun install`, or unpin before linking.
- Link never creates, changes, or removes an override.
- Unpin never creates an ephemeral link.
- Status warnings for linked packages exposed through another repository stay
  visible even when a consumer override mitigates selected resolution. The
  warning describes contaminated physical state.

## Output Contract

Pin and unpin share `effigy.deps.pin.v1` with an explicit `operation` field
inside the standard `effigy.command.v1` envelope.

Minimum fields:

- schema and schema version
- operation (`pin` or `unpin`)
- manager (`bun`)
- repo root, manifest path, and canonical library path
- dry-run flag and outcome
- package name, local path, before value, after value, and planned action
- writes actually performed
- portability or conflict warnings
- verification state
- next actions, including `bun install` after a manifest change

Required outcomes are `dry-run`, `applied`, `already-applied`, `no-match`,
`conflict`, and `apply-failed`. The implementation batch must add the JSON
schema, selection example, and schema-index entry with the command.

## Error Contract

Actionable failures name the consumer, library, manifest, affected packages,
and next action where known. They include:

- missing or unsupported library layout
- unsupported package manager
- invalid manifest or non-object `overrides`
- conflicting selected override
- active Effigy-managed link for the same library or package
- concurrent manifest change
- inability to preserve or atomically replace the manifest
- any observed Bun lockfile mutation

No error path may leave a partial package closure in the manifest.

## Ownership Boundary

- `effigy-cli` owns grammar, help, and parse errors.
- `effigy-deps` owns inventory reuse, package selection, override planning,
  layout-preserving manifest edits, atomic apply, verification, and typed
  reports.
- The runner owns root resolution, text rendering, the standard JSON envelope,
  and exit semantics.
- Contract artifacts own the versioned pin payload schema and selection proof.

The runner must not own package matching or ad hoc JSON text surgery.

## Acceptance

- Dry-run proves zero writes and reports the complete matched package set.
- Pin adds every selected package in one manifest mutation.
- Exact re-pin is a no-op.
- One conflicting entry blocks every planned addition.
- Unpin removes only exact local-library entries.
- Unrelated overrides and manifest formatting survive pin and unpin.
- No operation changes a Bun lockfile or another repository.
- Relative paths resolve from `--repo` when invoked elsewhere.
- Link and pin cannot create mixed ownership state.
- Text and JSON identify committed behavior and the required install step.
- A Soundcheck/Poodle consumer fixture proves the transitive override removes
  duplicate package identity after the operator runs Bun install.

## Out of Scope

- automatic install or lockfile editing
- manifest edits from `deps link`
- mutation of intermediate dependency repositories
- Cargo pinning
- generic override editing or version selection
- absolute or machine-specific committed paths
- package-explicit unpin without a library checkout
- committed pin state in the machine-local link ledger
- automatic CI checkout orchestration for sibling repositories

## Change Policy

Changes to grammar, package selection, conflict behavior, manifest ownership,
path portability, link interaction, write safety, or JSON shape require this
contract to change in the same batch.

## Next Task

No implementation work remains in this contract lane.
