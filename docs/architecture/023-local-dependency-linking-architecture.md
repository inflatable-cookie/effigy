# Local Dependency Linking Architecture

Status: active
Updated: 2026-08-05
Roadmaps: `g08.018` through `g08.023`
Contract: [`034`](../contracts/034-local-dependency-linking-contract.md)

## Purpose

Effigy needs one package-manager-aware surface for temporarily resolving a
consumer's dependencies from local library checkouts. The committed dependency
source remains authoritative; local edit-in-place development is an explicit,
machine-local overlay.

The first surface is:

```text
effigy deps
effigy deps status [cargo|bun]
effigy deps link <cargo|bun> <LIBRARY_PATH> [--dry-run]
effigy deps unlink <cargo|bun> <LIBRARY_PATH> [--dry-run]
```

`deps` is the domain. `link` and `unlink` describe the common state transition.
`cargo` and `bun` select the package manager and therefore the physical
mechanism.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: local dependency switching remains fast and reversible
  without weakening committed source identity or machine-readable command
  contracts.
- Vision target delta: Effigy gains a package-manager-aware dependency domain
  instead of leaving local overrides as per-repo shell ritual.

## System Inventory

| Surface | Authority |
| --- | --- |
| Consumer manifests | Committed dependency intent: Cargo git/tag sources and Bun package versions |
| Consumer lockfiles | Committed resolved baseline; never the desired-state store for local links |
| Library manifests | Package/crate names, workspace membership, package roots, and peer declarations |
| `.effigy/local/dependency-links.json` | Repo-local desired link state, ignored by git |
| `~/.effigy/deps/bun-registrations.json` | Machine-local ownership/refcount index for Effigy-managed Bun registrations |
| `.cargo/config.toml` | Cargo's repo-local physical patch mechanism, ignored by git |
| Bun global link registry | Bun's machine-global package registration mechanism |
| Consumer `node_modules` symlinks | Bun's ephemeral physical link mechanism |
| `effigy deps` | Inventory, planning, mutation, verification, status, and JSON reporting |
| `effigy doctor` | Independent hygiene and drift observer |

## Authority Map

- Effigy owns desired link state, managed markers, operation plans, verification,
  and actionable findings.
- Cargo owns dependency resolution through config-level `[patch]` tables.
- Bun owns package registration and save-less consumer symlinks.
- Library repositories own their crate/package inventories.
- Consumer repositories own committed manifests and lockfiles.
- Portfolio strategy documents provide adoption rationale and proof targets;
  Effigy's contract owns the shipped command behavior.

Effigy must not treat a local link as a manifest migration. Migration from path
dependencies to tagged or published dependencies is separate future work.

## Shared Operation Pipeline

Every manager follows the same observable pipeline:

1. resolve the consumer and canonical library path
2. inventory library packages
3. inventory the consumer dependency graph
4. match the full direct-and-transitive library closure
5. detect preconditions and conflicts
6. produce a deterministic plan
7. apply manager-specific changes unless `--dry-run`
8. verify actual resolution
9. persist desired state
10. report per-package before/after state

No-match, already-linked, and already-unlinked outcomes are reports, not
opportunities to write partial state.

## Desired State

The repo-local ledger records enough information to detect drift without
re-running a mutating operation. Effigy ensures the repo's `.effigy/` local
state is ignored and reports any ignore-file change.

- schema version
- package manager
- canonical library path
- affected consumer workspace roots
- matched package names and local package roots
- expected committed source identity
- manager mechanism
- ownership metadata needed for safe unlink

The ledger is desired state, not proof of current resolution. Status and doctor
compare it with `.cargo/config.toml`, Cargo resolution, Bun registrations, and
consumer symlinks.

Bun also needs a locked, atomic machine-local index at
`~/.effigy/deps/bun-registrations.json`. It records the canonical package path,
whether Effigy created the registration, and the consumer/link identities that
still desire it. Repo-local state alone cannot safely decide whether a global
registration is shared.

## Cargo Adapter

Cargo links use one config at the git/repo root. Canonical absolute paths make
the same patch visible to nested workspaces without per-workspace relative-path
ambiguity.

The adapter:

- inventories real Cargo workspaces through `cargo metadata`
- also finds workspace-less multi-crate layouts
- treats explicit workspace roots as boundaries, skips archived reference
  trees and orphaned descendant manifests, and uses locked metadata for
  planning/status so observation cannot rewrite locks
- observes committed resolution from outside the consumer config search tree
  when a compatible legacy patch makes the normal locked query stale
- matches library crates present in the consumer graph and groups them by exact
  declared git source URL
- patches the full matching closure inside Effigy-managed marker blocks
- adopts or directly removes a compatible pre-Effigy patch only when its table
  contains no unrelated crates and every path matches the requested checkout
- checks every exact planned before-state before the first write, then applies
  config and ignore files atomically
- preserves unrelated and hand-managed Cargo config
- refuses tracked config files, mixed legacy tables, and mismatched collisions
- refuses first-link mutation when an affected tracked `Cargo.lock` is dirty;
  later links allow only drift owned by already-active package closures
- verifies every workspace/crate pair with Cargo metadata and tree evidence
  before persisting desired state
- scopes post-plan verification and observation to persisted consumer roots so
  the repo-root patch does not touch unrelated nested workspace locks
- rolls back only exact config/ignore content applied by the failed operation;
  lockfiles are never restored destructively
- removes only the selected managed block on unlink, re-runs Cargo resolution,
  and proves exact Git sources plus clean or remaining-link-only lock state

Cargo may rewrite lockfiles while a patch is active. That is expected local
state and must be surfaced as do-not-commit state.

## Bun Adapter

Bun links use explicit `--no-save` on every `bun link`/`bun unlink` process;
the package manager's version-dependent save default is never trusted and
`--save` is forbidden.

The adapter:

- inventories root and workspace packages from library manifests
- matches the full package closure present in the consumer dependency tree
- registers each matched local package with Bun
- links every matched package into the consumer through one explicit
  full-closure invocation without manifest or lockfile changes
- removes consumer links through exact symlink target checks because Bun's
  `unlink` command unregisters the current package rather than accepting
  consumer package names
- invokes `bun unlink --no-save` only for a provably unshared Effigy-owned
  global registration
- records enough ownership to avoid unregistering another consumer's or a
  hand-created global registration
- verifies consumer symlinks resolve to the requested library paths
- detects missing links after `bun install`
- checks for duplicated framework peers such as Svelte across symlinked raw
  source packages

Bun symlinks are ephemeral. Re-running the same link operation repairs drift.
Complete loss of the desired symlink closure is repairable drift; a mixed
local/registry closure is a correctness error.

## Ownership Boundary

The target implementation should use a focused dependency domain owner shared
by the command and doctor surfaces. CLI parsing/help stays in `effigy-cli`;
command dispatch and rendering stay in the normal built-in/runner shell;
dependency inventory, plans, state, and manager verification stay below that
shell. `effigy-doctor` consumes read-only inspection rather than duplicating
manager parsing.

Dependency-direction review selected a focused `effigy-deps` crate. It remains
below CLI, doctor, and runner shells so both command and health surfaces can
consume one model without circular ownership.

## Expansion Boundary

The `deps` namespace deliberately leaves room for later dependency inspection,
health, and migration commands. The first tranche implements only status,
link, and unlink. It must not add passthrough wrappers for arbitrary Cargo or
Bun commands.

## Next Task

Use [`guide 077`](../guides/077-local-dependency-linking.md) for operation.
Future dependency scope requires a new explicit roadmap; none is inferred.
