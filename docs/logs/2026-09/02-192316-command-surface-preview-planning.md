# Command-Surface Preview Planning

Date: 2026-09-02
Status: Ready for execution
Roadmap: [`g09.001`](../../roadmaps/g09/001-command-surface-compaction-preview.md)
Spec: [`116`](../../specs/116-command-surface-compaction-preview-strict-lane.md)
Card: [`1109`](../../roadmaps/g09/batch-cards/1109-add-executable-command-namespaces.md)

## Outcome

Promoted Theme 4 into the first `g09` lane after operator confirmation of the
target taxonomy, `v1.0` removal gate, retained direct `watch`, warning shape,
legacy-help visibility, and additive-preview completion boundary.

## Consumer Inventory

The inventory covered 30 top-level repositories under
`/Users/tom/Dev/projects` with a root `effigy.toml`.

- No exposed bare task is named `local`, `repo`, `deliver`, `extend`, or
  `admin`.
- Compli-me has a catalog alias `admin`; its slash selectors are a named
  regression boundary.
- All 29 consumer repositories contain current direct command references.
- The bounded tracked-surface scan found 4,665 grouped-command tokens; 1,796
  remain after managed `.agents/**` copies are excluded.
- Highest-volume families were `release` (1,113), `graph` (693), `scan` (560),
  `docs` (360), `deps` (291), and `container` (276).

The numbers are impact bounds, not a manual-edit count. Historical logs,
handoffs, archived specs, old generations, dependencies, build output, and Git
metadata were excluded. Managed skills must move through the authoritative
Effigy source and supported distribution path.

## Decisions

- Direct daily spine: tasks/selectors, `tasks`, `test`, `watch`, `doctor`,
  `init`, help/version flags, and global target/output flags.
- Canonical namespaces: `local`, `repo`, `deliver`, `extend`, `admin`.
- Legacy direct built-ins remain executable until `v1.0`.
- Human warnings use stderr; JSON warnings are typed optional envelope metadata.
- Primary help and completion teach grouped routes; legacy detailed help remains
  with migration facts.
- The first lane ends after the additive preview. Removal is future gated work.

## Generation Rollover

All 48 `g08` roadmaps are Complete and strict specs `097`, `099`, `100`, and
`115` are archived. Draft spec `098` is archived as paused history rather than
silently entering this lane. `g09` therefore opens as an explicit operator-
selected planning era, not an automatic file-count rollover.

September contains the final `g08` evidence and first `g09` planning record.
The active-month log directory remains in place; historical records are not
moved or rewritten merely to split a calendar month.

## Validation

`effigy qa:docs` passed every docs, link, JSON-example, index, workflow-path,
and next-action check. `git diff --check` passed. `effigy doctor --json`
reported `ok: true`, zero errors, the known stale-graph warning, and seven
warning-level god-file findings.

## Vision Target Delta

- Tags: `ROUTE`, `CONTRACT`, `MAINT`, `RELEASE`
- Baseline: help groups existed but executable grammar remained flat.
- Current: the additive grouped migration has canonical authority, a Ready
  card, consumer inventory, compatibility gate, and review oracle.
- Open: implementation and exact-head review; later direct-route removal stays
  blocked on `v1.0` plus refreshed consumer evidence.

## Next Task

Execute ready card `1109`; do not infer a release or direct-route removal.
