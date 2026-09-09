# Roadmap Backlog Retirement

Status: complete
Created: 2026-09-09
Roadmap: none — `g09` closed
Batch: roadmap-backlog-retirement

## Summary

- Retired the `docs/roadmaps/backlog/` intake layer: all six files deleted
  and the directory removed. `find docs -type d -name backlog -print` returns
  nothing.
- Roadmaps hold only promoted executable tasks; `docs/triage/` temporarily
  holds unresolved or deferred candidates with no execution authority.
- Added the `docs/triage/README.md` non-authoritative intake contract.
- Moved the two surviving candidates to timestamped triage notes without
  change of meaning; neither is promoted into a task.
- Made guide `049` self-contained for versioning, rollback, support-window,
  and v1-planning policy.
- Retargeted every live inbound reference to current guides, triage notes, or
  the roadmaps front door; converted retired-path mentions in historical
  records to plain text.
- Added a `test ! -e docs/roadmaps/backlog` absence check to repo-owned
  `qa:docs` and the bundled Northstar starter; the starter also emits a
  `docs/triage/README.md` anchor.

## Disposition Manifest

| Former backlog file | Classification | Destination |
| --- | --- | --- |
| `docs/roadmaps/backlog/README.md` | obsolete scaffolding | deleted |
| `breaking-command-surface-and-container-compaction.md` | implemented and superseded | deleted; command and container authority already lives in current architecture, contracts, guides, and archived `g09` evidence |
| `distribution-channels.md` | implemented, promoted through `g03.020` | deleted; live references retargeted to guides `042`, `049`, `051`, and `062` |
| `release-contract-v0.md` | accepted durable design, mostly promoted | deleted; rollback communication, early-v0 support-window, and v1-planning criteria merged into guide `049` (§6a, §6d, §6e, §10) |
| `g09-candidate-themes.md` | completed or superseded except consumer cohort expansion | `docs/triage/20260909-152106-consumer-adoption-cohort-expansion.md` (links vision `007` and `020`); source deleted; Theme 5 not a candidate task |
| `vendored-effigy-skill-portfolio-status-and-sync.md` | unresolved deferred candidate | `docs/triage/20260909-152107-vendored-effigy-skill-portfolio-sync.md` (links the open `PAPERCUTS.md` entry, kept open); source deleted |

## Changes

- Deleted: all six files under `docs/roadmaps/backlog/` and the directory.
- Added: `docs/triage/README.md`,
  `docs/triage/20260909-152106-consumer-adoption-cohort-expansion.md`,
  `docs/triage/20260909-152107-vendored-effigy-skill-portfolio-sync.md`,
  `crates/effigy-catalog/starters/northstar/docs/triage/README.md`.
- Edited: `docs/roadmaps/README.md` (Triage section),
  `docs/roadmaps/archive/g02.md`, `g03.md`, `g08.md`,
  `docs/contracts/001-working-rules.md`,
  `docs/guides/014-release-checklist-template.md`,
  `docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md`,
  `docs/guides/044-distribution-first-publish-execution-runbook.md`,
  `docs/guides/049-ci-binary-distribution-and-release-protocol.md`,
  `docs/guides/056-northstar-effigy-consumer-repo-contract.md`,
  `docs/vision/002-refocus-matrix-v1.md`,
  `docs/vision/020-strategic-runway-atlas-v1.md`,
  `docs/vision/decisions/D-2026-03-horizon-a-governance-theme.md`,
  `docs/specs/archive/034-next-v0-x-readiness-and-roadmap-selection-strict-lane.md`,
  `PAPERCUTS.md`, `config/tasks.toml`,
  `crates/effigy-catalog/starters/northstar/starter.toml`,
  `crates/effigy-catalog/starters/northstar/effigy.toml`,
  `crates/effigy-catalog/starters/northstar/docs/README.md`,
  `crates/effigy-catalog/starters/northstar/docs/roadmaps/README.md`,
  `crates/effigy-catalog/src/starter.rs`,
  `crates/effigy-doctor/src/manifest_schema/tests.rs`,
  `src/tests/runner_tests/runner_core_tests/init_migrate_tests/init_tests.rs`,
  `CHANGELOG.md`.
- No edit needed: `docs/README.md` (no backlog reference),
  `docs/logs/README.md` (already paused with Atlas next),
  `docs/guides/078-papercuts-discovery-and-capture.md` (generic "backlog
  item" wording, not roadmap-backlog doctrine).

## Historical Exceptions

Historical logs, vision history, closed handoffs, immutable queue records,
and plain-text provenance keep naming the former paths without live links:

- `docs/logs/archive/2026-02/` and `2026-03/` evidence naming
  `docs/roadmaps/backlog/distribution-channels.md` and
  `release-contract-v0.md` as the authority of record at the time.
- `docs/specs/archive/034-*.md` closeout prose ("no backlog item is
  promoted", "the live `v0.x` contract still governs").
- `docs/roadmaps/archive/g09.md` deferred-items prose and its
  `vision/007-cross-repo-rollout-and-adoption-posture-v1.md` link predate this
  cutover and are outside the owned mutation surfaces.
- `docs/handoffs/20260909-152106-retire-roadmap-backlog.md` transport handoff;
  deleted in the queue closeout commit, not preserved as documentation.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `OPERATE`
- Movement: two intake layers (executable roadmap tasks plus roadmap backlog)
  -> one clean boundary (executable `gNN.NNN` tasks only, non-authoritative
  triage intake)
- Remaining gap: next strategic runway is operator-owned (Northstar Atlas);
  no active generation, no approved frontier

## Validation Performed

- `find docs -type d -name backlog -print`
  - result: empty (exit 0, `wc -l` 0)
- `test ! -e docs/roadmaps/backlog`
  - result: pass; also exercised directly as the new
    `qa:docs:backlog-absence` task (exit 0) and mirrored in the starter as
    `qa:northstar:no-backlog`
- `cargo run --bin effigy -- qa:docs`
  - result: pass (exit 0), including links, examples, index, agent-defaults,
    backlog-absence, and the full vision bundle (headings, contains,
    workflow-paths, vision index, next-action)
- `cargo test -p effigy-catalog northstar_starter -- --nocapture`
  - result: 2 passed (emitted file set with `docs/triage/README.md`,
    absence-check task bundle)
- `cargo test -p effigy-doctor manifest_schema -- --nocapture`
  - result: 27 passed (fixture retargeted to guide `049`)
- `cargo test -p effigy --lib init_northstar`
  - result: 5 passed (emitted triage anchor)
- `cargo test --test cli_output_tests northstar_starter_profile_is_queryable`
  - result: 1 passed
- `cargo fmt --all -- --check`
  - result: clean
- `git diff --check`
  - result: clean

## Risks

- Archived roll-ups `g02`, `g03`, and `g08` were edited in place to keep live
  links working; their historical narrative is otherwise untouched.
- The `test ! -e` absence check relies on POSIX `test` negation semantics,
  which hold for `/bin/test` as well as shell builtins.

## Next Task

Use Northstar Atlas with the operator to choose the next strategic runway. Do
not open `g10` or authorize execution from this archived log.
