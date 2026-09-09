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
- [`2026-09/`](./archive/2026-09/) — 36 logs (`g08` final closeout, `g09`, and flattened-task switchover)

Vision rollout history is summarized in `docs/vision/history/README.md`.

## Active logs

- none — the latest generation is closed and its log window is archived

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

`g09` is closed with no ready lane. Use Northstar Atlas with the operator to
choose the next strategic runway. Effigy release remains a separate
operator-gated mutation.
