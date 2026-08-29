# Local Dependency Linking Contract

Status: Active
Owner: Platform maintainers
Roadmaps: `g08.018` through `g08.023`

## Purpose

Define Effigy's machine-local dependency linking behavior across Cargo and Bun.
Committed manifests remain the truth for clean clones, CI, releases, and other
developers. Local links may redirect resolution for edit-in-place development
but must remain discoverable, reversible, and safe to keep out of commits.

## Terminology

- **consumer**: the repo in which `effigy deps` is run
- **library**: the local checkout named by `LIBRARY_PATH`
- **package manager**: `cargo` or `bun`; this selects the physical mechanism
- **dependency link**: desired state in which the consumer resolves the
  matching library closure from the local checkout
- **closure**: every crate or package from that library present in the
  consumer graph, direct or transitive
- **drift**: desired link state exists but physical resolution no longer
  matches it

## Command Grammar

```text
effigy deps
effigy deps status [cargo|bun]
effigy deps link <cargo|bun> <LIBRARY_PATH> [--dry-run]
effigy deps unlink <cargo|bun> <LIBRARY_PATH> [--dry-run]
```

- Bare `effigy deps` is equivalent to `effigy deps status`.
- The standard global `--repo <PATH>` and `--json` behavior applies.
- `--dry-run` is accepted only by `link` and `unlink` and performs no writes,
  link registrations, symlink changes, or lockfile resolution.
- `cargo` and `bun` are package-manager identifiers, not language names.
- Future package managers require explicit contract and fixture coverage.

## Shared Invariants

- Never modify `Cargo.toml`, `package.json`, or any other committed dependency
  manifest.
- Every Bun link/unlink invocation passes explicit `--no-save`; never rely on
  Bun's version-dependent save default and never invoke `--save`.
- Match and redirect the full library closure present in the consumer graph.
- Support multiple local libraries simultaneously.
- Link and unlink are idempotent.
- Re-link refreshes desired state and repairs manager drift.
- Unlink removes only state Effigy owns for that consumer/library/manager.
- An unlinked unlink is a successful no-op with a note.
- A library with no matching consumer dependencies produces a no-match report
  and no writes.
- A consumer still using path dependencies produces a pre-migration report and
  no link writes for those dependencies.
- Text and JSON report the plan, package-level before/after state, warnings,
  and verification verdict.

## Desired-State Ledger

Effigy records repo-local desired state under:

```text
.effigy/local/dependency-links.json
```

The ledger must be covered by the repo's `.effigy/` local-state ignore rule.
Effigy adds that rule when absent and reports the ignore-file delta. The ledger
must be versioned, deterministic, and updated atomically. It records manager,
canonical library path, consumer roots, matched packages, expected committed
sources, physical mechanism, and safe-unlink ownership.

Status must tolerate and report:

- ledger missing while an Effigy-managed Cargo block exists
- ledger entry whose library path no longer exists
- manager state missing after a previously successful link
- foreign or hand-managed state that Effigy does not own

## Machine-Local Bun Registration Index

Effigy records Bun registration ownership and desired consumers under:

```text
~/.effigy/deps/bun-registrations.json
```

The index is versioned, locked for concurrent operations, and updated
atomically. Each entry records package name, canonical package path, whether
Effigy created the registration, and the repo/link identities that still
desire it. Newly created index and lock files use owner-only permissions on
Unix.

- Never claim a registration that already existed, even when it points at the
  requested path.
- Never replace a registration that points at another path.
- Remove a registration only when Effigy created it, its observed target still
  matches, and no desired consumer reference remains.
- Validate other repo references before removal. Retain and report stale or
  unverifiable references rather than risk breaking another consumer.
- Status and doctor report index/registry disagreement as drift.

## Cargo Rules

### Inventory and matching

- Resolve the library path canonically.
- Use Cargo metadata to enumerate workspace packages.
- Support workspace-less multi-crate repositories by discovering and
  inspecting standalone package manifests outside ignored/build directories.
- Inspect every Cargo workspace under the consumer repo, including nested
  workspaces in a single git repository.
- Treat explicit `[workspace]` manifests as discovery boundaries. Do not run
  metadata separately for their member or orphaned descendant manifests, and
  ignore archived `reference/` and `references/` trees.
- Planning, dry-run, status, and doctor use locked Cargo metadata so observation
  cannot rewrite a lockfile. An unrelated workspace blocked only by stale
  locked resolution may be skipped when none of its local manifests declares a
  target-library crate. When a compatible repo-local patch itself makes locked
  resolution stale, repeat the locked query outside the consumer config search
  tree to observe the committed graph without applying that patch or rewriting
  the lockfile.
- Match library crates against resolved consumer packages and their exact
  declared git source URLs.
- Patch every matching crate from the library present in the graph. Partial
  patching is a correctness error.

### Physical state

- Use one repo-root `.cargo/config.toml` with canonical absolute library paths.
- Write entries under `[patch."<exact-url>"]` inside clearly delimited
  Effigy-managed blocks.
- Preserve unrelated tables, comments, and hand-managed entries.
- `link` may adopt a pre-Effigy patch table only when every entry belongs to
  the requested library and resolves to its canonical local crate path.
  `unlink` may remove that same compatible legacy table directly when the
  explicit library path proves the target. Both operations refuse mixed tables.
- Refuse a `.cargo/config.toml` already tracked by git.
- Refuse mismatched same-source/same-crate patches instead of overwriting them.
- Create the config when absent and ensure `.cargo/config.toml` is ignored by
  the repo; report any `.gitignore` change to the operator.
- Remove an empty config file and empty `.cargo/` directory on unlink only when
  Effigy created them and no unrelated content remains.

### Lockfile safety

- Before the first link, refuse if any affected tracked `Cargo.lock` is dirty.
  When links are already active, allow only drift confined to packages owned by
  those desired links; refuse unrelated lock changes before writing.
- Link warns that Cargo verification/builds may rewrite lock entries to path
  sources.
- Post-link verification, status, and unlink inspect only the persisted
  consumer workspace roots. A repo-root patch must not cause unrelated nested
  workspace lockfiles to be resolved as an observation side effect.
- Active linked lock state is do-not-commit state.
- Unlink re-resolves the committed git/tag source non-destructively; it must not
  run `git checkout`, `git restore`, or discard unrelated changes.
- If other desired links remain, report their exact package-owned lock drift.
  Otherwise the affected lockfile must return byte-for-byte to the tracked
  baseline. Any unrelated drift is a failed operation.

### Verification

- After link, Cargo metadata/tree evidence must resolve each matched crate from
  the expected local path.
- After unlink, the same packages must resolve from the committed git source.
- Verification failure leaves an explicit failed/drifted report; it must not be
  rendered as success.

## Bun Rules

### Inventory and matching

- Enumerate a root package and workspace packages declared by the library,
  including the portfolio's `packages/*/package.json` layout.
- Match direct and transitive packages from that library in the consumer
  dependency tree.
- Link the full matching package closure. Partial local/registry duplication is
  a correctness error.

### Physical state

- Register matched packages by running `bun link --no-save` in their package
  directories.
- Link the complete matched closure into the consumer in one invocation:
  `bun link <PACKAGE_1> ... <PACKAGE_N> --no-save`. Per-package consumer
  invocations are unsafe because Bun may re-resolve or replace an earlier
  linked member while processing the next package.
- Snapshot `package.json` and Bun lockfile state before and after. Any mutation
  is a failed operation and must be reported.
- Track whether a global Bun registration was created by Effigy, already
  existed for the same path, or conflicts with a different path.
- Never replace a foreign registration silently.
- Bun has no consumer-side `unlink <PACKAGE>` command. Unlink consumer
  symlinks directly only after exact path/target revalidation; never remove a
  registry directory or foreign symlink.
- Use `bun unlink --no-save` only in a local package directory, and only to
  remove a global registration Effigy created whose last desired reference is
  being released. Retain and report every shared, foreign, stale, or
  unverifiable registration.

### Drift and peers

- Bun links are ephemeral across installs. Missing or redirected consumer
  symlinks are drift.
- Status inspects `file:` dependencies declared by the consumer root and its
  selected workspaces. When the target repository's visible `node_modules`
  resolution exposes a package symlink whose target is outside that
  repository, report the dependency name, package name, link path, and target.
  Ignore Bun store links and workspace links that stay inside the target
  repository.
- Re-link repairs missing symlinks idempotently.
- Verify each linked package resolves to the expected canonical library path.
- Detect duplicate framework peer resolution for symlinked raw-source packages,
  including Svelte. Report the package paths and dedupe remediation.
- Manifest/lock mutation, mixed local/registry partial closure, or conflicting
  package paths are errors. Complete loss of the desired symlink closure is a
  warning until repaired.

## Status and Doctor

`effigy deps` and `effigy deps status` report desired and observed state for
both managers, optionally filtered by manager. Bare status also discovers root
package managers from committed manifests (`Cargo.toml`; Bun via `package.json`
plus lockfile, Bun `packageManager`, or workspaces when `packageManager` is
absent) and reports them in `detected_managers`. An explicit non-Bun
`packageManager` is not treated as Bun even when `workspaces` is present. When
exactly one root manager is detected and no filter is passed, JSON `manager` is
that manager instead of `null`.

Status also reports committed local dependencies as observed links. A Cargo
`path` dependency or a Bun `file:`/`link:` specifier whose declared target
resolves to a directory outside the checkout is grouped by the containing
library checkout and reported with `committed_local` set and `desired` absent.
These reports are observations, not ledger entries: state is `healthy`, the
single drift reason is informational, and neither `deps link` nor `deps unlink`
acts on them. Declarations that do not resolve are left to the package manager.

Minimum per-link fields:

- manager
- mechanism (`cargo-patch` or `bun-link` for a managed link;
  `cargo-path-dependency` or `bun-file-dependency` for a committed local)
- library path
- consumer roots
- package names
- desired state, or committed-local identity
- observed state
- drift reasons
- lockfile or manifest hygiene
- verification summary

Doctor behavior:

- healthy active links: informational
- committed path or `file:` local dependency in force: informational
- missing library path or partial closure: error
- tracked local Cargo config or hand-managed collision: error
- Cargo lockfile carrying active path-link resolution: error with
  do-not-commit remediation
- complete Bun link loss after install: warning with re-link remediation
- external package link exposed through a cross-repository Bun `file:`
  dependency: warning with unlink-or-override remediation
- mixed local/registry Bun closure or registration-index conflict: error
- Bun manifest/lock mutation or duplicate incompatible peer resolution: error

Doctor must remain read-only.

## JSON Contract

All `deps` operations emit the standard `effigy.command.v1` envelope under
global `--json`. Status uses `effigy.deps.status.v1`; Cargo link uses
`effigy.deps.link.v1`; Cargo and Bun unlink use `effigy.deps.unlink.v1`.
Operation payloads include the exact plan, manager-specific physical intent,
resolution/verification evidence, immutable state, outcome, and rollback
report. Later manager operations must add or extend an explicit versioned
dependency schema. Schema and selection indexes change in the same
implementation batch as the command.

## Error Contract

Actionable errors include:

- library path missing or not a supported package layout
- requested package manager unavailable
- consumer has no matching dependencies
- consumer still declares matching path dependencies
- dirty Cargo lockfile before link
- tracked or conflicting Cargo config
- Bun global registration points at another path
- verification does not resolve the full closure locally/remotely as expected

Errors name the manager, consumer root, library path, affected packages, and
next action where known.

## Related Committed Surface

Contract [`040`](./040-bun-committed-dependency-pinning-contract.md) defines an
accepted, separate Bun pin/unpin surface for committed consumer overrides. It
does not weaken this contract:

- link and unlink remain save-less and manifest-immutable
- link never creates, changes, or removes an override
- pin state never enters the local-link ledger or Bun registration index
- overlapping committed and machine-local state must be reported and refused,
  not silently converted

Pin/unpin remains unavailable until its own roadmap batch is implemented and
validated.

## Out of Scope

- Editing dependency manifests.
- Publishing crates or packages.
- Choosing or bumping dependency versions.
- Generic passthrough wrappers for Cargo/Bun commands.
- Implementing future `deps check`, `deps audit`, or `deps migrate` commands in
  this tranche.
- Supporting package managers other than Cargo and Bun.

## Change Policy

Changes to command grammar, manager semantics, state location, closure rules,
manifest/lock invariants, or JSON payload shape require a contract update in
the same change.

## Drift Triggers

- Cargo patch/config discovery behavior changes.
- Bun link registration or save-less behavior changes.
- A new manager is added.
- The dependency desired-state ledger moves or changes schema.
- Doctor severity or lock/peer hygiene policy changes.

## Next Task

Use [`guide 077`](../guides/077-local-dependency-linking.md) for operation.
Committed pinning proceeds separately under roadmap `g08.031` and contract
`040`. Do not fold that work into link semantics.
