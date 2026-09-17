# Bounded Doctor and Incremental Scan Architecture

Status: active
Updated: 2026-09-17

## Problem

`effigy doctor` currently mixes structural repository diagnosis with multiple
full-tree content scans and the repository-defined `health` task. Large
monorepos therefore pay for repeated walks plus an unbounded task invocation.
Cold non-interactive runs can appear silent until an outer caller times out.

Caching alone cannot bound that cold path. The command needs a cheap default
tier, an explicit expensive tier, one catalog-scoped inventory, and a deadline
that produces useful partial evidence.

## Command tiers

`effigy doctor` is the fast structural tier. It checks manifest preparation,
composition conflicts, environment tools, task references, draft lifecycle,
graph-index state, dependency health, and runtime diagnostics. It does not walk
repository content for scan findings and does not execute `health`.

`effigy doctor --deep` adds content scans and the selected scope's `health`
task. This is an explicit request for repository work, not a compatibility
alias for the old unbounded behavior. `--refresh` is deep-only and bypasses
cache reads while preserving atomic publication.

`doctor <task>` remains explanation mode. It does not combine with deep,
catalog, fan-out, or refresh flags.

## Scope selection

Doctor uses effective catalog membership rather than discovering another
monorepo topology:

- an invocation inside a member selects that effective catalog;
- `--catalog <alias>` selects one declared effective member;
- a root invocation selects the root scope and prunes effective member roots;
- `--all-catalogs` explicitly fans out across root plus declared members;
- declared external members are valid explicit or fan-out scopes.

One-scope work must not walk, run health in, or write cache state for a sibling.
Catalog aliases and canonical roots identify findings and cache ownership.

## Shared inventory

Each selected scope is walked once per deep invocation. The inventory applies
the effective ignore posture and streams reusable observations to all enabled
scanners. It must not retain all source text merely to share the walk.

The existing god-file, comment-ratio, generated-asset,
generated-in-source, attention-marker, duplicate-block, and stale-suppression
evaluators consume that shared observation stream when enabled. Their finding
semantics and deterministic ordering remain compatible with the standalone
scan surfaces.

## Incremental cache

Disposable workspace-owned cache state lives under
`.effigy/doctor/cache/v1/<encoded-scope>/`. Entries contain per-file reusable
facts, not rendered doctor output. Facts may include line/code/comment counts,
generated classification, marker hits, suppression evidence, and normalized
duplicate fingerprints.

Cache identity includes:

- canonical repository and scope identity;
- effective catalog topology;
- scan configuration digest;
- cache schema and scanner implementation versions;
- ignore posture;
- exact file identity.

Clean tracked files may use the Git index blob identity. Dirty, untracked, and
non-Git files use a content digest computed during inventory. Size and mtime
alone are never sufficient. Warm runs still establish an exact inventory but
do not reread or reanalyse unchanged content.

Publication is locked and atomic. An incompatible or corrupt entry is ignored
and rebuilt with a warning. Cancellation never replaces a previously valid
cache generation with partial state.

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

Text and JSON report the selected mode and scopes, effective budget, total
duration, completeness, and each check's duration and state:

- `complete`
- `cached`
- `skipped`
- `budget-exhausted`

Deep scan reports also expose cache hit/miss counts. Non-interactive callers
receive the same terminal evidence as interactive callers; correctness never
depends on a spinner.

## Compatibility boundary

Moving scans and `health` behind `--deep` deliberately narrows the default
command and requires release-note treatment. Existing scan evaluators,
standalone scan commands, `--fix` structural repair behavior, catalog routing,
and task execution semantics remain owned by their current surfaces. Deep mode
does not grant additional mutation authority.

## Non-goals

- a background daemon or watcher;
- automatic catalog discovery;
- remote or shared cache storage;
- automatic cache pruning policy;
- changing scan thresholds or finding semantics;
- making repository `health` safe by silently ignoring its failure;
- workflow or release mutation.
