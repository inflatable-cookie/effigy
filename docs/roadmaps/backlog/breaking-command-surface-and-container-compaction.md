# Breaking Command Surface and Container Compaction

Status: Live
Owner: Platform
Created: 2026-05-09
Depends on: `v0.5.0` release closeout

## 1) Purpose

Define the next breaking cleanup lane after `v0.5.0`.

This backlog item has two linked goals:

- tighten the root command surface so helper-style built-ins stop competing
  with real domain roots
- reduce container crate sprawl so one crate owns the container domain

This is backlog work, not the active strict lane. Do not execute it by
drifting inside `g04.019`.

## 2) Decisions Already Made

Keep at root:

- `init`
- `scan`
- `test`
- `watch`
- `doctor`
- `tasks`
- `config`

Move in the next breaking release:

- `migrate` -> `tasks migrate`
- `unlock` -> `tasks unlock`
- `cache` -> `tasks cache`
- `completion` -> `config completion`

Remove:

- `catalogs`

Rationale:

- `init` is intended to grow into a general repo-init system, not stay a task
  helper
- `scan` reads as a real operator surface, not task plumbing
- `migrate`, `unlock`, and root `cache` are task-domain maintenance
- `completion` belongs under `config`
- `catalogs` is only an alias and should not survive a breaking tidy-up

## 3) Goals

- [ ] Make the root help surface read like product domains, not a mixed bag of
  helper entrypoints.
- [ ] Remove ambiguous root names where nested homes are clearer.
- [ ] Keep migration scope bounded and well documented for the next breaking
  release.
- [ ] Collapse the three container crates into one canonical container-domain
  crate.
- [ ] Reassess the smaller execution/runtime support crates after the container
  merge instead of compacting everything blindly.

## 4) Non-Goals

- [ ] Do not move `init` under `tasks`.
- [ ] Do not move `scan` under `tasks` or `doctor`.
- [ ] Do not mix this cleanup with new container/runtime features.
- [ ] Do not open a new generation or replace the active `g04.019` strict lane
  just to land this work.

## 5) Command Surface Target

### Before

- `effigy migrate`
- `effigy unlock`
- `effigy cache ...`
- `effigy completion ...`
- `effigy catalogs`

### After

- `effigy tasks migrate`
- `effigy tasks unlock`
- `effigy tasks cache ...`
- `effigy config completion ...`
- no `effigy catalogs`

### Required Cleanup

- builtin registry and dispatch
- root help topics
- command reference matrix
- shell completion command index
- parser and help tests
- migration notes and release notes
- stale alias coverage

## 6) Crate Compaction Target

### Phase A: Container Domain Merge

Merge:

- `effigy-container-manager`
- `effigy-container-ops`

into:

- `effigy-containers`

Target shape:

- `effigy-containers` becomes the one canonical container-domain crate
- container models, policy, operation planning, compose/backend resolution,
  runtime wrappers, reporting, and workspace integration live together
- top-level `effigy` stops depending on three overlapping container crates

### Phase B: Follow-On Reassessment

Reassess after the container merge:

- `effigy-context`
- `effigy-execution`
- `effigy-runtime-plan`

Do not pre-commit to merging those crates. Reassess based on what the
container merge exposes.

### Explicit Hold

Leave these alone unless a later audit shows a concrete boundary problem:

- `effigy-bootstrap`
- `effigy-data`
- `effigy-manifest`
- `effigy-catalog`
- `effigy-gateway`
- `effigy-rhai`
- `effigy-state`
- `effigy-release`
- `effigy-distribution`
- `effigy-doctor`

## 7) Execution Plan

### Batch 1 - Breaking Command Surface

- [x] add nested homes for `tasks migrate`, `tasks unlock`, `tasks cache`
- [x] add `config completion`
- [x] remove root `migrate`, `unlock`, `cache`, `completion`, `catalogs`
- [x] update root help and migration notes

### Batch 2 - Command Contract Cleanup

- [x] update docs, examples, and shell completion surfaces
- [x] update parser/help/reference tests
- [x] update release-note migration guidance

### Batch 3 - Container Crate Merge

- [x] fold `effigy-container-manager` into `effigy-containers`
- [x] fold `effigy-container-ops` into `effigy-containers`
- [x] remove old crate references from the workspace graph
- [x] keep the merge behavior-preserving

### Batch 4 - Post-Merge Audit

- [x] reassess `effigy-context`, `effigy-execution`, `effigy-runtime-plan`
- [x] decide whether one more compaction lane is justified
- [x] stop if the remaining seams still represent real boundaries

## 8) Acceptance Criteria

- [x] root help no longer advertises helper-style built-ins that have better
  nested homes
- [x] migration notes show exact before/after commands
- [x] no surviving `catalogs` alias
- [x] container-domain code no longer spans three overlapping crates
- [x] workspace metadata and top-level dependencies reflect the smaller crate
  graph
- [x] release notes for the next breaking version can explain the command
  changes in one short section

## 9) Risks and Mitigations

- [ ] Risk: breaking command moves fragment docs and completions.
  - Mitigation: land command moves with help/docs/tests/completion updates in
    the same batch.
- [ ] Risk: container crate merge becomes a refactor swamp.
  - Mitigation: keep it behavior-preserving and delay any second-wave crate
    compaction until after the first merge is stable.
- [ ] Risk: root `cache` rename is confused with `container cache`.
  - Mitigation: use the migration notes to call out that these are different
    surfaces with different homes.

## 10) Promotion Criteria

Primary tags:

- `ROUTE`
- `MAINT`

Target envelope:
- Effigy exposes a tighter root CLI and a smaller, clearer container-domain
  crate graph without widening product scope.

Promotion signals:

- one approved breaking-release plan includes exact before/after command
  examples and a bounded migration note set
- one container-domain audit confirms `effigy-containers` can absorb the other
  two container crates without introducing a new circular or confused boundary
- the active generation has a real execution window for this cleanup instead of
  treating it as open-ended background work

## 12) Batch 4 Audit Result

Post-merge reassessment conclusion:

- keep `effigy-context`
- keep `effigy-execution`
- keep `effigy-runtime-plan`

Why:

- `effigy-context` is still a small, clean authority seam for cwd/root capture,
  host facts, and container-handoff detection
- `effigy-runtime-plan` is still a small, clean activation-plan seam for route,
  readiness, alias, and lease policy
- `effigy-execution` is larger, but it is also broadly consumed across runner,
  Rhai, and CLI dispatch surfaces, so it still represents a real boundary
  rather than namespace churn

Current judgment:

- no immediate follow-on compaction lane is justified for these three crates
- if `effigy-execution` grows materially beyond its current dispatch/request
  role, reassess it separately instead of bundling it into a broad crate merge

## 11) Pickup Note

Current state of Batch 1:

- partial command-surface implementation is already in the worktree
- root parser and builtin registry have been shifted toward:
  - `tasks migrate`
  - `tasks unlock`
  - `tasks cache`
  - `config completion`
- root `catalogs` alias has been removed from parser and builtin dispatch
- help, command matrix, completion index, and a wide set of tests have been
  partially updated to the new shapes

Not done yet:

- broad validation has not been completed
- crate compaction has not started
- expect more fallout in parser/help/JSON-contract/CLI-output tests before this
  batch is stable

Resume sequence:

- run `cargo fmt --all`
- run `git diff --check`
- run focused validation first, not full gates:
  - parser/help tests
  - builtin/help contract tests
  - CLI output tests touching `tasks`, `config completion`, and removed
    `catalogs`
- only start the container-crate merge after the command-surface batch is
  green
