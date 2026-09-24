# Logs

Logs capture execution evidence, checkpoints, release notes, and sweeps.

## Segmentation model

- Group logs by month directory: `YYYY-MM/`
- Name each log: `DD-HHMMSS-<slug>.md`

Imported historical logs were normalized from older date-first filenames during the Northstar migration.

Examples:
- `2026-02/26-090200-effigy-extraction-and-migration-checkpoint.md`
- `2026-03/10-090000-script-surface-unification-batch-1.md`

## Thread logs

When a feature spans multiple same-day checkpoints, add a consolidation log that links those checkpoints and provides one final validation matrix.

## Cadence rule

- Create logs per completed batch or update cycle.
- Do not create a separate log for every task.

## Governance reviews

Monthly governance reviews use template
`docs/vision/009-vision-governance-review-template-v1.md`.
Store them under `docs/logs/<month>/` with a `vision-governance-review` slug.
Reference the artifact register and decision index from
[`docs/vision/governance/`](../vision/governance/).

## Vision Target Delta Requirement

All new logs that act as release or validation reports should include a `## Vision Target Delta` section that states:

- primary vision tags touched (`ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`, `RELEASE`)
- what moved in this report (baseline -> current state)
- what remains open (or `None`)

Forward-only policy cutoff:

- logs dated on or after `2026-03-06` must include `## Vision Target Delta`
- logs before `2026-03-06` are not required to be backfilled

Historical workflow-reference exception:

- logs may keep historical workflow paths (for example `.github/workflows/*.yml`) when they document what existed at the time
- do not rewrite historical log evidence only to match current repo layout
- active docs outside `docs/logs/` must use current workflow paths (`.github/workflows/*.yml`)

Historical command-surface exception:

- logs may mention retired wrapper scripts or older command names when they
  document what existed at the time
- treat references such as `scripts/check-release-gates.sh`,
  `scripts/check-release-install-from-tag.sh`, `scripts/check-release-smoke.sh`,
  `scripts/install-local-bin-links.sh`, and `scripts/prepare-release.sh` as
  historical evidence only, not current operator guidance
- active docs outside `docs/logs/` must point at the current native Effigy
  surfaces instead

## Retention and archival convention

- The **active** log window is the current generation's month directory
  (`docs/logs/<current-month>/`). It is indexed below.
- Logs for **closed generations** are moved under `docs/logs/archive/<month>/`
  and dropped from this index. They remain in the repository (and in git
  history) as durable evidence; this index just stops carrying every entry.
- Never delete a log to compact. Move it to `archive/` and let the per-month
  directory stand as the record. Roadmap `Evidence` links into archived
  months keep working via the `logs/archive/<month>/` path.
- When a generation closes, archive its month directories in the same sweep
  that closes its roadmaps, then trim this index to the active window.

## Archived logs

Closed-generation logs live under [`archive/`](./archive/):

- [`2026-02/`](./archive/2026-02/) — 32 logs
- [`2026-03/`](./archive/2026-03/) — 149 logs
- [`2026-04/`](./archive/2026-04/) — 256 logs
- [`2026-05/`](./archive/2026-05/) — 219 logs
- [`2026-06/`](./archive/2026-06/) — 20 logs (`g08` opening tranche)
- [`2026-08/`](./archive/2026-08/) — 46 logs (`g08` close-out tranche)
- [`2026-09/`](./archive/2026-09/) — 37 logs (`g08` final closeout, `g09`, flattened-task switchover, and roadmap-backlog retirement)

Vision rollout history is summarized in `docs/vision/history/README.md`.

## Active logs

- [`2026-09/24-164500-g10-014-compatible-dependabot-lock.md`](./2026-09/24-164500-g10-014-compatible-dependabot-lock.md)
  — current-base Cargo.lock refresh for six compatible Dependabot updates
- [`2026-09/24-162800-dependabot-cargo-frontier.md`](./2026-09/24-162800-dependabot-cargo-frontier.md)
  — promoted ten Dependabot PRs into two serial reviewed Cargo batches
- [`2026-09/24-152221-effigy-v0-13-0-release-note.md`](./2026-09/24-152221-effigy-v0-13-0-release-note.md)
  — 0.13.0 release notes, migration guidance, and publication verification
- [`2026-09/13-100503-named-skill-resolution-stdio-planning.md`](./2026-09/13-100503-named-skill-resolution-stdio-planning.md)
  — opened g10 with one ready agent-native skill-execution task
- [`2026-09/13-104533-named-skill-resolution-stdio-closeout.md`](./2026-09/13-104533-named-skill-resolution-stdio-closeout.md)
  — closed g10.001 after merge, review, synchronization, and canonical closeout
- [`2026-09/13-164131-northstar-refresh.md`](./2026-09/13-164131-northstar-refresh.md)
  — reconciled g10.002 lifecycle completion across current planning front doors
- [`2026-09/14-082254-effigy-papercut-frontier-planning.md`](./2026-09/14-082254-effigy-papercut-frontier-planning.md)
  — promoted two independent reliability repairs and closed one stale candidate
- [`2026-09/15-071120-catalog-scoped-code-graph-planning.md`](./2026-09/15-071120-catalog-scoped-code-graph-planning.md)
  — promoted catalog-derived segmented graph indexing and independent storage
- [`2026-09/15-080920-dependency-maintenance-unblock-planning.md`](./2026-09/15-080920-dependency-maintenance-unblock-planning.md)
  — isolated base Cargo supply-chain repair from the reviewed graph PR
- [`2026-09/15-170538-published-and-draft-task-planning.md`](./2026-09/15-170538-published-and-draft-task-planning.md)
  — promoted compatible published and provisional task surfaces
- [`2026-09/17-183923-bounded-doctor-planning.md`](./2026-09/17-183923-bounded-doctor-planning.md)
  — promoted bounded structural doctor and incremental catalog-scoped deep scans
- [`2026-09/24-122316-g10-front-door-reconciliation.md`](./2026-09/24-122316-g10-front-door-reconciliation.md)
  — reconciled g10.010 terminal state and indexed two open Acowtancy triage notes
- [`2026-09/24-124833-release-readiness-audit.md`](./2026-09/24-124833-release-readiness-audit.md)
  — audited unreleased runtime and docs changes; recorded pre-release blockers
- [`2026-09/24-130018-release-hardening-frontier.md`](./2026-09/24-130018-release-hardening-frontier.md)
  — promoted four audit findings into three ready pre-0.13.0 repairs
- [`2026-09/24-133017-g10-011-closeout-currentness-repair.md`](./2026-09/24-133017-g10-011-closeout-currentness-repair.md)
  — removed planning currentness drift holding g10.011 lifecycle closeout
- [`2026-09/24-134221-g10-013-closeout-currentness-repair.md`](./2026-09/24-134221-g10-013-closeout-currentness-repair.md)
  — cleared stale next actions holding g10.013 lifecycle closeout
- [`2026-09/24-141915-g10-012-closeout-currentness-repair.md`](./2026-09/24-141915-g10-012-closeout-currentness-repair.md)
  — cleared the final repair from live next actions for closeout
- [`2026-09/24-145155-release-gate-graph-cwd-repair.md`](./2026-09/24-145155-release-gate-graph-cwd-repair.md)
  — repaired captured graph cwd after the release QA drift guard failed

## Log template

```md
# <Log Title>

Status: complete
Created: YYYY-MM-DD
Roadmap: gNN.NNN
Batch: <batch-slug>

## Summary
- ...

## Changes
- ...

## Vision Target Delta
- Primary tags: `...`
- Movement: baseline `...` -> current `...`
- Remaining gap: `...` (or `None`)

## Validation Performed
- command: `...`
  - result: ...

## Risks
- ...

## Next Task
- ...
```

## Next Task

Dispatch ready `g10.014` through Northstar Queue, then `g10.015` after its
terminal closeout. Future release mutation and doctor cache pruning remain
separate decisions.
