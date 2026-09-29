# Bounded Doctor and Scan Cache Contract

Owner: doctor, scan, catalog-routing, and task-execution maintainers

## Purpose

This contract makes doctor bounded and useful in large monorepos without
weakening diagnosis. It separates cheap structural health from explicit deep
work, reuses one exact catalog-scoped inventory, and defines safe incremental
cache and timeout behavior.

## CLI contract

1. `effigy doctor` runs only the structural tier. It never executes repository
   `health` and never performs a content-scan tree walk.
2. `effigy doctor --deep` runs structural checks, enabled content scans, and
   the selected scope's `health` task.
3. `--catalog <alias>` and `--all-catalogs` are deep-only and mutually
   exclusive. `--refresh` is deep-only.
4. `doctor <task>` explanation mode retains existing behavior and rejects
   deep, catalog, fan-out, and refresh combinations before work begins.
5. `--fix` retains its current structural repair boundary. Deep mode creates
   no new mutation permission.

## Catalog scope contract

1. Scope comes only from the composed manifest's effective catalog membership.
2. Root scope prunes all effective member roots.
3. Cwd member selection or `--catalog` touches only that member.
4. `--all-catalogs` is the only implicit root-plus-members fan-out.
5. Declared external members follow the same rules and keep workspace-owned
   cache state keyed by canonical scope identity.
6. A selected scope's inventory, cache, findings, and `health` execution cannot
   read or mutate a sibling scope.

## Health precondition contract

1. Before deep doctor runs a selected scope's `health` task, it checks that the
   scope's declared JS dependency bootstrap is present. The check uses the
   scope manifest's declared `[package_manager].js` and the manager's committed
   lock evidence; it is never a universal `node_modules` rule. A scope is
   bootstrapped only when its install contains every declared runtime,
   development, and peer dependency (optional dependencies stay out because
   their absence can be a valid platform outcome), which keeps running
   `health` even when the scope has no lock of its own. An empty or partial
   `node_modules` is not an install, because a missing declared package could
   still resolve from an ancestor. A symlinked `node_modules` counts only when
   its resolved target is a directory that stays inside the local install
   boundary; a link to an ancestor checkout or to an in-boundary file is
   treated as missing and cannot smuggle in a parent install.
   Each declared package needs readable `package.json` metadata with a
   nonempty package name. Both its resolved directory and metadata must stay
   inside the authorized install boundary. Empty entries and package links
   into a standalone child's parent are missing dependencies. No `index.js`
   or runtime entry point is required: exports-only and type-only packages
   are valid. Manager store links inside the local boundary remain valid;
   verified workspace members may link into their workspace store, including
   isolated layouts, without borrowing root-only dependency entries.
2. When the scope has no valid local install, the required install root is the
   nearest ancestor with the selected manager's lock file, bounded by the
   scope's repository boundary and the workspace root. A lock outside those
   boundaries never satisfies the scope, so a scope with its own repository
   boundary cannot be satisfied by an ancestor lock or install.
3. A scope whose `package.json` declares dependencies and that has no valid
   local install emits a `health.task.bootstrap` error naming
   `effigy bootstrap deps sync <path>` and the missing declared dependencies,
   and does not run that scope's `health` task. The reported reason is
   `missing-local-lock` when no lock exists anywhere inside the scope's
   repository boundary, including a standalone child repository with no child
   lock.
4. An ancestor lock or install only satisfies the scope when the ancestor
   declares the scope as a JS workspace member in the manager-authoritative
   source: `package.json` `workspaces` for Bun and npm, or
   `pnpm-workspace.yaml` `packages` for pnpm. The two sources are never
   unioned, so a pnpm exclusion cannot be re-included by a `package.json`
   pattern. Membership patterns do not match paths nested below a declared
   workspace package. An ancestor lock without verified workspace membership
   is reported as a gap that names the foreign lock; doctor never treats it
   as the scope's local install.
5. Even a verified workspace member only shares the ancestor install when the
   selected manager's checked-in layout hoists member dependencies.
   - npm hoists by default; `.npmrc` `install-strategy = nested | shallow |
     linked` keeps dependencies member-local.
   - Bun uses the hoisted linker unless the effective linker is isolated: an
     explicit `bunfig.toml` `[install] linker`, or a `configVersion = 1`
     lockfile in a workspace, selects isolated. Under hoisting, a
     self-contained member keeps its own install via its `package.json`
     `installConfig.hoistingLimits = "workspaces"` or the root
     `workspaces.selfContained` list of member paths or package names.
   - pnpm's default isolated layout keeps each project's dependencies under
     that project's own `node_modules` unless `.npmrc` sets
     `node-linker=hoisted`.

   A member whose layout requires a local install and has none is reported as
   `missing-local-install` even when the workspace root is installed.
6. A scope whose `package.json` declares no dependencies and a catalog without
   a declared JS package manager keep running `health` unchanged.

## Budget contract

1. Fast mode defaults to 10,000 milliseconds overall. Deep mode defaults to
   120,000 milliseconds overall.
2. `EFFIGY_DOCTOR_TIMEOUT_MS` replaces that default. `0` means unbounded and
   must be visible in the report.
3. One monotonic deadline is propagated through checks, per-file work, and
   health execution. A phase cannot reset it.
4. Budget exhaustion returns non-zero, marks the report incomplete, identifies
   the phase, preserves completed evidence, and gives a useful retry action.
5. A timed-out health task has its owned process tree terminated and reaped.
   No health side effect may continue after doctor exits.

## Inventory and cache contract

1. Deep mode performs at most one physical content walk per selected scope.
2. Enabled content scanners consume one shared ordered observation stream.
3. Cached facts must reproduce the same findings and ordering as uncached
   evaluation and standalone scan commands.
4. Cache keys include canonical repo/scope identity, catalog topology, scan
   config, ignore posture, schema version, implementation version, and exact
   file identity.
5. Git index blob identity is valid only for a clean tracked file. All other
   files require a content digest. Mtime plus size cannot establish a hit.
6. Cache publication under `.effigy/doctor/cache/v1/` is locked and atomic.
   Interrupted work cannot publish partial state or destroy a valid prior
   generation.
7. Corrupt or incompatible cache content is ignored, reported, and rebuilt.
8. `--refresh` bypasses reads and atomically replaces successful entries.
9. Cache content is disposable local state and is never required in version
   control.

## Report contract

Human and JSON reports expose:

- mode, selected scopes, configured budget, elapsed time, and completeness;
- per-check `complete`, `cached`, `skipped`, or `budget-exhausted` state;
- per-check duration;
- deep cache hit, miss, and invalid-entry counts;
- timeout phase and next steps when incomplete.

JSON changes are additive unless the existing schema contract requires an
explicit version bump. A spinner may improve interactive feedback but cannot
carry evidence absent from the terminal report.

## Compatibility and migration

Moving content scans and repository `health` out of default doctor is a
documented behavior change. Release notes and doctor help must point consumers
needing the old breadth to `effigy doctor --deep`.

Standalone scan commands, scan thresholds, finding semantics, catalog aliases,
task resolution, health task execution semantics, and current JSON fields stay
compatible unless this contract explicitly adds data.

## Required proof

- A synthetic large repository proves default doctor performs no content walk
  and never invokes a health sentinel within the 10-second default budget.
- An instrumented cold deep run proves exactly one walk per selected scope
  while all enabled scanners retain standalone-equivalent findings.
- A warm deep run proves unchanged files are not reread or reanalysed.
- Same-size, same-mtime changed content misses the cache.
- One selected catalog does not touch sibling files, cache, or health; explicit
  all-catalog fan-out visits each declared scope once.
- Deadline expiry during inventory and health produces partial evidence,
  non-zero exit, no partial cache publication, and no surviving child process.
- A child scope with its own lock and no local `node_modules` reports the
  missing bootstrap and never invokes its health sentinel; after a local
  bootstrap install the same scope runs health. A standalone child repository
  with its own git boundary, no child lock, no valid local install, and an
  ancestor lock/install is reported as `missing-local-lock` and never runs
  health; the same holds when the child's `node_modules` is a symlink into
  that ancestor install or to an in-boundary file. A complete child-local
  `node_modules` directory runs health even without a child lock, while an
  empty or partial child install that leaves a declared package resolvable
  only from an ancestor is reported as `missing-local-install` with the
  missing dependency names. A hoisted
  declared workspace member sharing an ancestor install still runs health,
  while a self-contained Bun member, a Bun `configVersion = 1` isolated
  member, a pnpm isolated member, and an npm
  `install-strategy = nested | shallow | linked` member without their own
  install are reported as `missing-local-install`. A scope whose ancestor lock
  lacks verified membership, a non-JS catalog, and a dependency-free child are
  each covered.
- Corrupt cache input rebuilds safely with a useful warning.
- Explain mode, structural `--fix`, text output, and JSON consumers retain
  their defined behavior.

## Drift triggers

Revisit this contract when doctor tier membership, catalog selection, timeout
defaults, cache identity/schema, scan observation semantics, process-tree
cancellation, or doctor JSON completeness fields change.
