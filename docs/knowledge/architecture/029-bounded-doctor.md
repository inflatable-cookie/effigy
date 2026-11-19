# Bounded Doctor and Shared Scan Inventory Architecture


## Problem

`effigy doctor` currently mixes structural repository diagnosis with multiple
full-tree content scans and the repository-defined `health` task. Large
monorepos therefore pay for repeated walks plus an unbounded task invocation.
Cold non-interactive runs can appear silent until an outer caller times out.

A cheaper default tier bounds that cold path: an explicit expensive tier, one
catalog-scoped inventory, and a deadline that produces useful partial evidence.

## Command tiers

`effigy doctor` is the fast structural tier. It checks manifest preparation,
composition conflicts, environment tools, task references, draft lifecycle,
graph-index state, dependency health, and runtime diagnostics. It does not walk
repository content for scan findings and does not execute `health`.

Manifest shape findings come from the production manifest grammar:
`effigy_manifest` is asked about each table, so doctor reports a key or value
exactly when the loader refuses it and a finding names the narrowest refused key
path. There is no second schema table beside the loader.

`effigy doctor --deep` adds content scans and the selected scope's `health`
task. This is an explicit request for repository work, not a compatibility
alias for the old unbounded behavior, and it keeps no scan state between runs.

`doctor <task>` remains explanation mode. It does not combine with deep,
catalog, or fan-out flags.

## Scope selection

Doctor uses effective catalog membership rather than discovering another
monorepo topology:

- an invocation inside a member selects that effective catalog;
- `--catalog <alias>` selects one declared effective member;
- a root invocation selects the root scope and prunes effective member roots;
- `--all-catalogs` explicitly fans out across root plus declared members;
- declared external members are valid explicit or fan-out scopes.

One-scope work must not walk or run health in a sibling. Catalog aliases and
canonical roots identify findings.

Deep doctor guards each selected scope's `health` task with the scope's own JS
bootstrap posture. A local `node_modules` satisfies the precondition only when
it contains every declared runtime, development, and peer dependency; an empty
or partial install does not, because a missing package could still resolve
from an ancestor. A complete local install keeps running health, even without
a child lock. Otherwise the selected
manifest's declared `[package_manager].js` and the nearest in-boundary
committed lock identify the required install root, and a scope that lacks a
complete local or verified shared install emits a `health.task.bootstrap`
finding and skips its health task, so Bun or Node cannot silently resolve a
package
from an ancestor checkout. A symlinked
`node_modules` only counts as a local install when its resolved target is a
directory inside the local install boundary, so a link into the parent or to
an in-boundary file cannot bypass the guard. A scope with its own git boundary
cannot be satisfied by an ancestor install; a standalone child repository with
declared dependencies, no complete local install, and no in-boundary lock is
reported as `missing-local-lock` even when an ancestor provides a lock and
install. An
ancestor lock or install only satisfies the scope when the ancestor declares
the scope in the manager-authoritative workspace source (`package.json`
`workspaces` for Bun/npm, `pnpm-workspace.yaml` `packages` for pnpm), without
matching nested paths below a declared package; the sources are never unioned.
Even a verified member shares the ancestor install only when the manager's
effective layout hoists member dependencies. Bun selects the isolated linker
from `bunfig.toml` `[install] linker` or a `configVersion = 1` workspace
lockfile, and under hoisting a self-contained member (`package.json`
`installConfig.hoistingLimits = "workspaces"` or the root
`workspaces.selfContained` list of member paths or names) needs its own
install. pnpm's default isolated layout needs the member's own `node_modules`
unless `.npmrc` sets `node-linker=hoisted`, and npm needs a member-local
install under `install-strategy = nested | shallow | linked`.
An unverified ancestor lock is reported as a gap. Non-JS and dependency-free
scopes are unaffected.

Installed-package evidence is readable `package.json` metadata with a
nonempty name, with both the package directory and metadata resolved inside
the authorized install boundary. Empty package entries and parent-resolving
package links fail the guard. No particular runtime entry point is required.
Local manager store links remain valid, and verified workspace members may
link into their workspace store even under isolated layouts.

## Shared inventory

Each selected scope is walked once per deep invocation. The inventory applies
the effective ignore posture and streams reusable observations to all enabled
scanners. It must not retain all source text merely to share the walk.

The god-file and duplicate-block evaluators consume that shared observation
stream when enabled. Their finding semantics and deterministic ordering remain
compatible with the standalone scan surfaces. v0.15 removed the other five
scanners (Q-009), so the inventory no longer records comment counts, generated
asset reasons, generated-in-source categories, or marker/suppression hits.

## No persistent scan state

Deep doctor keeps no cache. Every deep run reads the selected scope's files,
derives the shared observations, and discards them when the run ends. Nothing
is written under `.effigy/doctor/`, no Git index identity subprocess runs, and
an interrupted run leaves no partial scan state.

The cache was removed in v0.16. A consumer's cold deep run on a 42,880-file
repository took 56 seconds with no cache hits, because the generation exceeded
its own 128 MiB bound and never published; the persistent forms, locks and
eviction therefore bought nothing at the scale they were meant for. Running
`effigy doctor --deep` therefore has the same cost as its first run.

## Time budget and cancellation

Fast doctor has a 10-second overall default budget. Deep doctor has a
120-second overall default budget. `EFFIGY_DOCTOR_TIMEOUT_MS` overrides the
selected default; `0` disables it deliberately.

The remaining deadline flows through inventory, scan evaluation, and health
execution. Work cooperatively stops between files. Health uses the existing
interrupt-aware process ownership so deadline expiry terminates the child
process tree and waits for cleanup; it must not leave detached work running.

Budget exhaustion returns a non-zero result with completed evidence, the
in-progress phase, elapsed time, and next steps. It is not converted into a
warning-only success.

## Reporting

The repository's cold doctor process fixtures reserve all configured test
slots in the default nextest profile. These proofs require the intended phase
and child readiness inside their original 800 ms or 1,000 ms deadline; running
them alongside other process-heavy tests can exhaust the budget before that
proof begins. The reservation changes runner scheduling, not production
deadlines, readiness assertions, retries or selection. Unrelated tests remain
parallel. It does not guarantee scheduling against arbitrary external host
load. `test:doctor:cold-scheduling` covers the captured default-profile proof
family.

Nextest's terminal status includes failed, slow, skipped and leaky identities;
ordinary passing-case chatter is suppressed. This keeps diagnostic identities
near the retained QA tail without changing test results or the leak policy.
The retained step logs still have a size limit and are not a complete-stream
guarantee. Historical discarded identities cannot be reconstructed from it.

Text and JSON report the selected mode and scopes, effective budget, total
duration, completeness, and each check's duration and state:

- `complete`
- `skipped`
- `budget-exhausted`

The v0.16 report dropped the deep cache counters and the `cached` state.
Non-interactive callers receive the same terminal evidence as interactive
callers; correctness never depends on a spinner.

## Compatibility boundary

Moving scans and `health` behind `--deep` deliberately narrows the default
command and requires release-note treatment. Existing scan evaluators,
standalone scan commands, `--fix` structural repair behavior, catalog routing,
and task execution semantics remain owned by their current surfaces. Deep mode
does not grant additional mutation authority.

## Non-goals

- a background daemon or watcher;
- automatic catalog discovery;
- any persistent or shared scan cache;
- changing scan thresholds or finding semantics;
- making repository `health` safe by silently ignoring its failure;
- workflow or release mutation.
