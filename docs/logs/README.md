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

Vision rollout history is summarized in `docs/vision/history/README.md`.

## Active logs

- [`2026-09/01-150452-no-match-benchmark-isolation-1098.md`](./2026-09/01-150452-no-match-benchmark-isolation-1098.md)
- [`2026-09/01-135932-markdown-frontmatter-1097.md`](./2026-09/01-135932-markdown-frontmatter-1097.md)
- [`2026-09/01-133154-catalog-fragment-listing-1096.md`](./2026-09/01-133154-catalog-fragment-listing-1096.md)
- [`2026-09/01-123424-papercuts-env-lock-audit.md`](./2026-09/01-123424-papercuts-env-lock-audit.md)
- [`2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md`](./2026-09/01-095641-catalog-pack-acquisition-prototype-1095.md)
- [`2026-09/01-092640-catalog-pack-acquisition-prototype-planning.md`](./2026-09/01-092640-catalog-pack-acquisition-prototype-planning.md)
- [`2026-09/01-080923-rhai-profile-independent-limits-1094.md`](./2026-09/01-080923-rhai-profile-independent-limits-1094.md)
- [`2026-09/01-075717-rhai-profile-limits-papercut-planning.md`](./2026-09/01-075717-rhai-profile-limits-papercut-planning.md)
- [`2026-08/31-233000-help-first-command-discovery-1093.md`](./2026-08/31-233000-help-first-command-discovery-1093.md)
- [`2026-08/31-213000-northstar-profile-proof-1090.md`](./2026-08/31-213000-northstar-profile-proof-1090.md)
- [`2026-08/31-181957-documentation-context-1089.md`](./2026-08/31-181957-documentation-context-1089.md)
- [`2026-08/31-162015-external-skill-task-runner-closeout.md`](./2026-08/31-162015-external-skill-task-runner-closeout.md)
- [`2026-08/31-151155-external-skill-task-runner-planning.md`](./2026-08/31-151155-external-skill-task-runner-planning.md)
- [`2026-08/30-164636-documentation-instruction-help-refresh-planning.md`](./2026-08/30-164636-documentation-instruction-help-refresh-planning.md)
- [`2026-08/30-174452-documentation-instruction-help-parity-closeout.md`](./2026-08/30-174452-documentation-instruction-help-parity-closeout.md)
- [`2026-08/30-004016-documentation-graph-1088.md`](./2026-08/30-004016-documentation-graph-1088.md)
- [`2026-08/29-233709-documentation-graph-profile-planning.md`](./2026-08/29-233709-documentation-graph-profile-planning.md)
- [`2026-08/27-173611-northstar-agents-rust-audit.md`](./2026-08/27-173611-northstar-agents-rust-audit.md)
- [`2026-08/21-230738-documentation-coverage-parity-closeout.md`](./2026-08/21-230738-documentation-coverage-parity-closeout.md)
- [`2026-08/21-224918-documentation-coverage-parity-planning.md`](./2026-08/21-224918-documentation-coverage-parity-planning.md)
- [`2026-08/18-112147-doctor-secrets-schema-parity-closeout.md`](./2026-08/18-112147-doctor-secrets-schema-parity-closeout.md)
- [`2026-08/17-153000-vision-governance-operationalization-closeout.md`](./2026-08/17-153000-vision-governance-operationalization-closeout.md)
- [`2026-08/12-094017-bun-pin-lockfile-fallback-closeout.md`](./2026-08/12-094017-bun-pin-lockfile-fallback-closeout.md)
- [`2026-08/12-090342-bun-pin-lockfile-fallback-planning.md`](./2026-08/12-090342-bun-pin-lockfile-fallback-planning.md)
- [`2026-08/11-234531-bun-pin-consumer-proof-and-closeout.md`](./2026-08/11-234531-bun-pin-consumer-proof-and-closeout.md)
- [`2026-08/11-232228-bun-pin-cli-json-and-interlocks.md`](./2026-08/11-232228-bun-pin-cli-json-and-interlocks.md)
- [`2026-08/11-224711-bun-pin-domain-foundation.md`](./2026-08/11-224711-bun-pin-domain-foundation.md)
- [`2026-08/11-182709-pre-release-ci-proof-closeout.md`](./2026-08/11-182709-pre-release-ci-proof-closeout.md)
- [`2026-08/11-173550-v011-pre-release-hardening-sweep.md`](./2026-08/11-173550-v011-pre-release-hardening-sweep.md)
- [`2026-08/11-144402-unified-test-orchestration-v011-closeout.md`](./2026-08/11-144402-unified-test-orchestration-v011-closeout.md)
- [`2026-08/10-105636-explicit-catalog-membership-closeout.md`](./2026-08/10-105636-explicit-catalog-membership-closeout.md)
- [`2026-08/10-104558-delete-discovery-and-align-diagnostics.md`](./2026-08/10-104558-delete-discovery-and-align-diagnostics.md)
- [`2026-08/10-101827-explicit-catalog-routing-cutover.md`](./2026-08/10-101827-explicit-catalog-routing-cutover.md)
- [`2026-08/10-095639-explicit-catalog-schema-foundation.md`](./2026-08/10-095639-explicit-catalog-schema-foundation.md)
- [`2026-08/09-164830-papercuts-discovery-and-capture.md`](./2026-08/09-164830-papercuts-discovery-and-capture.md)
- [`2026-08/06-223813-patch-release-candidate-proof.md`](./2026-08/06-223813-patch-release-candidate-proof.md)
- [`2026-08/06-223205-prepared-source-drift-policy.md`](./2026-08/06-223205-prepared-source-drift-policy.md)
- [`2026-08/06-222825-loopback-test-state-isolation.md`](./2026-08/06-222825-loopback-test-state-isolation.md)
- [`2026-08/06-120729-annotated-release-tag-integrity.md`](./2026-08/06-120729-annotated-release-tag-integrity.md)
- [`2026-08/06-111534-initial-current-version-release-tag.md`](./2026-08/06-111534-initial-current-version-release-tag.md)
- [`2026-08/05-231121-dependency-linking-suite-closeout.md`](./2026-08/05-231121-dependency-linking-suite-closeout.md)
- [`2026-08/05-230446-bun-closure-drift-repair-proof.md`](./2026-08/05-230446-bun-closure-drift-repair-proof.md)
- [`2026-08/05-225229-signal-cargo-portfolio-proof.md`](./2026-08/05-225229-signal-cargo-portfolio-proof.md)
- [`2026-08/05-222527-dependency-health-doctor-closeout.md`](./2026-08/05-222527-dependency-health-doctor-closeout.md)
- [`2026-08/05-221113-dependency-health-status-parity.md`](./2026-08/05-221113-dependency-health-status-parity.md)
- [`2026-08/05-215153-bun-unlink-peer-diagnostics-closeout.md`](./2026-08/05-215153-bun-unlink-peer-diagnostics-closeout.md)
- [`2026-08/05-212620-bun-link-apply-verification.md`](./2026-08/05-212620-bun-link-apply-verification.md)
- [`2026-08/05-212619-bun-full-closure-planning.md`](./2026-08/05-212619-bun-full-closure-planning.md)
- [`2026-08/05-201254-cargo-unlink-and-milestone-closeout.md`](./2026-08/05-201254-cargo-unlink-and-milestone-closeout.md)
- [`2026-08/05-172006-cargo-link-apply-verification.md`](./2026-08/05-172006-cargo-link-apply-verification.md)
- [`2026-08/05-165735-cargo-full-closure-planning.md`](./2026-08/05-165735-cargo-full-closure-planning.md)
- [`2026-08/05-163456-deps-cli-json-foundation-closeout.md`](./2026-08/05-163456-deps-cli-json-foundation-closeout.md)
- [`2026-08/05-162005-read-only-dependency-inventory-status.md`](./2026-08/05-162005-read-only-dependency-inventory-status.md)
- [`2026-08/05-155727-dependency-domain-state-foundation.md`](./2026-08/05-155727-dependency-domain-state-foundation.md)
- [`2026-06/05-080226-g08-roadmap-consolidation.md`](./2026-06/05-080226-g08-roadmap-consolidation.md)
- [`2026-06/04-233355-dead-code-final-burn-down.md`](./2026-06/04-233355-dead-code-final-burn-down.md)
- [`2026-06/04-232355-dead-code-rust-impl-call-precision.md`](./2026-06/04-232355-dead-code-rust-impl-call-precision.md)
- [`2026-06/04-232018-dead-code-rust-impl-call-planning.md`](./2026-06/04-232018-dead-code-rust-impl-call-planning.md)
- [`2026-06/04-231646-dead-code-data-shape-root-precision.md`](./2026-06/04-231646-dead-code-data-shape-root-precision.md)
- [`2026-06/04-230940-dead-code-data-shape-root-planning.md`](./2026-06/04-230940-dead-code-data-shape-root-planning.md)
- [`2026-06/04-230651-dead-code-descriptor-root-precision.md`](./2026-06/04-230651-dead-code-descriptor-root-precision.md)
- [`2026-06/04-230026-dead-code-descriptor-root-planning.md`](./2026-06/04-230026-dead-code-descriptor-root-planning.md)
- [`2026-06/04-225805-dead-code-trait-surface-precision.md`](./2026-06/04-225805-dead-code-trait-surface-precision.md)
- [`2026-06/04-225013-dead-code-trait-surface-planning.md`](./2026-06/04-225013-dead-code-trait-surface-planning.md)
- [`2026-06/04-223151-dead-code-test-scope-filter.md`](./2026-06/04-223151-dead-code-test-scope-filter.md)
- [`2026-06/04-222829-dead-code-residual-planning.md`](./2026-06/04-222829-dead-code-residual-planning.md)
- [`2026-06/04-221542-dead-code-scan-rust-signal-correction.md`](./2026-06/04-221542-dead-code-scan-rust-signal-correction.md)
- [`2026-06/04-215614-boundary-dead-code-self-adoption.md`](./2026-06/04-215614-boundary-dead-code-self-adoption.md)
- [`2026-06/04-214831-selected-duplicate-block-follow-through.md`](./2026-06/04-214831-selected-duplicate-block-follow-through.md)
- [`2026-06/04-214009-repo-marker-root-rule-convergence.md`](./2026-06/04-214009-repo-marker-root-rule-convergence.md)
- [`2026-06/04-212126-container-up-phase-boundary-cleanup.md`](./2026-06/04-212126-container-up-phase-boundary-cleanup.md)
- [`2026-06/04-210845-rhai-feature-descriptor-seam.md`](./2026-06/04-210845-rhai-feature-descriptor-seam.md)
- [`2026-06/04-210225-command-surface-descriptor-seam.md`](./2026-06/04-210225-command-surface-descriptor-seam.md)
- [`2026-06/04-204300-code-quality-boundary-sweep-lane-opened.md`](./2026-06/04-204300-code-quality-boundary-sweep-lane-opened.md)

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

Return to official catalog-pack publication planning under contract `043`.
Ranking, timeout, release/workflow, S3, and rollover work stay out of this
closed card's scope.
