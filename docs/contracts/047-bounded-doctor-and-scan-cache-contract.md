# Bounded Doctor and Scan Cache Contract

Status: active
Updated: 2026-09-17
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
- Corrupt cache input rebuilds safely with a useful warning.
- Explain mode, structural `--fix`, text output, and JSON consumers retain
  their defined behavior.

## Drift triggers

Revisit this contract when doctor tier membership, catalog selection, timeout
defaults, cache identity/schema, scan observation semantics, process-tree
cancellation, or doctor JSON completeness fields change.
