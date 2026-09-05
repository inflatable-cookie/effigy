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

Vision rollout history is summarized in `docs/vision/history/README.md`.

## Active logs

- [`2026-09/05-151227-docs-compaction-sweep.md`](./2026-09/05-151227-docs-compaction-sweep.md)
- [`2026-09/05-133718-docs-context-exact-identifier-1114.md`](./2026-09/05-133718-docs-context-exact-identifier-1114.md)
- [`2026-09/05-113123-docs-context-latency-and-freshness-1113.md`](./2026-09/05-113123-docs-context-latency-and-freshness-1113.md)
- [`2026-09/05-105500-release-gate-diagnosability-1112.md`](./2026-09/05-105500-release-gate-diagnosability-1112.md)
- [`2026-09/03-014518-acowtancy-consumer-replay-1111.md`](./2026-09/03-014518-acowtancy-consumer-replay-1111.md)
- [`2026-09/03-010246-vision-governance-review-cycle-2.md`](./2026-09/03-010246-vision-governance-review-cycle-2.md)
- [`2026-09/02-224606-flat-command-execution-1110.md`](./2026-09/02-224606-flat-command-execution-1110.md)
- [`2026-09/02-222056-flat-command-execution-planning.md`](./2026-09/02-222056-flat-command-execution-planning.md)
- [`2026-09/02-205536-command-surface-preview-1109.md`](./2026-09/02-205536-command-surface-preview-1109.md)
- [`2026-09/02-192316-command-surface-preview-planning.md`](./2026-09/02-192316-command-surface-preview-planning.md)
- [`2026-09/02-185453-catalog-pack-publication-and-cutover-closeout.md`](./2026-09/02-185453-catalog-pack-publication-and-cutover-closeout.md)
- [`2026-09/02-155016-official-catalog-pack-update-1107.md`](./2026-09/02-155016-official-catalog-pack-update-1107.md)
- [`2026-09/02-144609-catalog-pack-generated-baseline-1106.md`](./2026-09/02-144609-catalog-pack-generated-baseline-1106.md)
- [`2026-09/02-003915-catalog-pack-first-publication-authority-1105.md`](./2026-09/02-003915-catalog-pack-first-publication-authority-1105.md)
- [`2026-09/01-234606-catalog-pack-repository-foundation-1104.md`](./2026-09/01-234606-catalog-pack-repository-foundation-1104.md)
- [`2026-09/01-202830-catalog-pack-support-floor-1103.md`](./2026-09/01-202830-catalog-pack-support-floor-1103.md)
- [`2026-09/01-201505-catalog-pack-publication-promotion-and-runway.md`](./2026-09/01-201505-catalog-pack-publication-promotion-and-runway.md)
- [`2026-09/01-184159-docs-context-time-budget-1101.md`](./2026-09/01-184159-docs-context-time-budget-1101.md)
- [`2026-09/01-173500-child-catalog-suite-registry-1100.md`](./2026-09/01-173500-child-catalog-suite-registry-1100.md)
- [`2026-09/01-182838-rhai-storage-create-only-1099.md`](./2026-09/01-182838-rhai-storage-create-only-1099.md)
- [`2026-09/01-172541-docs-context-traversal-budget-1102.md`](./2026-09/01-172541-docs-context-traversal-budget-1102.md)
- [`2026-09/01-175827-parallel-papercuts-frontier-planning.md`](./2026-09/01-175827-parallel-papercuts-frontier-planning.md)
- [`2026-09/01-150452-no-match-benchmark-isolation-1098.md`](./2026-09/01-150452-no-match-benchmark-isolation-1098.md)
- [`2026-09/01-135932-markdown-frontmatter-1097.md`](./2026-09/01-135932-markdown-frontmatter-1097.md)
- [`2026-09/01-133154-catalog-fragment-listing-1096.md`](./2026-09/01-133154-catalog-fragment-listing-1096.md)
- [`2026-09/01-123424-papercuts-env-lock-audit.md`](./2026-09/01-123424-papercuts-env-lock-audit.md)
- [`2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md`](./2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md)
- [`2026-09/01-092640-catalog-pack-acquisition-prototype-planning.md`](./2026-09/01-092640-catalog-pack-acquisition-prototype-planning.md)
- [`2026-09/01-080923-rhai-profile-independent-limits-1094.md`](./2026-09/01-080923-rhai-profile-independent-limits-1094.md)
- [`2026-09/01-075717-rhai-profile-limits-papercut-planning.md`](./2026-09/01-075717-rhai-profile-limits-papercut-planning.md)

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

Cards `1112` through `1115` are complete; evidence is at
[`05-105500`](./2026-09/05-105500-release-gate-diagnosability-1112.md),
[`05-113123`](./2026-09/05-113123-docs-context-latency-and-freshness-1113.md),
and
[`05-133718`](./2026-09/05-133718-docs-context-exact-identifier-1114.md).
Card `1115` (cross-repository source routing, spec `122`) is complete; its
evidence is indexed above.
Card `1111`'s Acowtancy replay evidence is at
[`03-014518`](./2026-09/03-014518-acowtancy-consumer-replay-1111.md); the
consumer maturity question is settled (`007` section 6, 2026-09-05). Acowtancy
stays read-only; Effigy release remains a separate operator-gated mutation.

- [`2026-09/05-152400-cross-repository-source-routing-1115.md`](./2026-09/05-152400-cross-repository-source-routing-1115.md)
