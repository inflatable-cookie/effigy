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
- active docs outside `docs/logs/` must use current workflow paths (`.github-bak/workflows/*.yml` in this repository layout)

## Recent Release Notes

- [`../guides/036-release-notes-authoring-template-and-examples.md`](../guides/036-release-notes-authoring-template-and-examples.md) (authoring template + examples)
- [`2026-02-28-dag-watch-onboarding-release-note.md`](./2026-02/28-090000-dag-watch-onboarding-release-note.md)
- [`2026-02-28-json-envelope-removal-release-note.md`](./2026-02/28-090800-json-envelope-removal-release-note.md)
- [`2026-02-28-doctor-explain-mode-release-note.md`](./2026-02/28-090100-doctor-explain-mode-release-note.md)

## Recent Docs IA Logs

- [`2026-03-01-documentation-ia-completion-report.md`](./2026-03/01-091100-documentation-ia-completion-report.md)

## Vision Program History

See `docs/vision/history/README.md` for archived vision rollout checklists and closeout records.

## Recent Validation Logs

- [`2026-03-01-effigy-caching-phase-1-validation.md`](./2026-03/01-091200-effigy-caching-phase-1-validation.md)
- [`2026-03-01-shell-completion-and-command-discovery-validation.md`](./2026-03/01-091900-shell-completion-and-command-discovery-validation.md)
- [`2026-03-01-completion-candidates-phase-2-validation.md`](./2026-03/01-090600-completion-candidates-phase-2-validation.md)
- [`2026-03-01-completion-candidates-memoization-validation.md`](./2026-03/01-090500-completion-candidates-memoization-validation.md)
- [`2026-03-01-completion-candidates-cache-state-validation.md`](./2026-03/01-090200-completion-candidates-cache-state-validation.md)
- [`2026-03-01-completion-candidates-cache-age-validation.md`](./2026-03/01-090000-completion-candidates-cache-age-validation.md)
- [`2026-03-01-completion-candidates-cache-ttl-validation.md`](./2026-03/01-090400-completion-candidates-cache-ttl-validation.md)
- [`2026-03-01-completion-candidates-cache-manifest-digest-validation.md`](./2026-03/01-090100-completion-candidates-cache-manifest-digest-validation.md)
- [`2026-03-01-completion-candidates-cache-ttl-override-validation.md`](./2026-03/01-090300-completion-candidates-cache-ttl-override-validation.md)
- [`2026-03-02-completion-candidates-cache-policy-telemetry-validation.md`](./2026-03/02-090200-completion-candidates-cache-policy-telemetry-validation.md)
- [`2026-03-02-completion-candidates-cache-policy-env-invalid-validation.md`](./2026-03/02-090000-completion-candidates-cache-policy-env-invalid-validation.md)
- [`2026-03-02-completion-candidates-cache-policy-json-contract-validation.md`](./2026-03/02-090100-completion-candidates-cache-policy-json-contract-validation.md)
- [`2026-03-02-completion-candidates-json-contract-docs-validation.md`](./2026-03/02-090900-completion-candidates-json-contract-docs-validation.md)
- [`2026-03-02-completion-candidates-json-examples-policy-delta-validation.md`](./2026-03/02-091000-completion-candidates-json-examples-policy-delta-validation.md)
- [`2026-03-02-docs-qa-completion-candidates-json-example-check-validation.md`](./2026-03/02-092500-docs-qa-completion-candidates-json-example-check-validation.md)
- [`2026-03-02-docs-qa-reports-index-check-validation.md`](./2026-03/02-092700-docs-qa-reports-index-check-validation.md)
- [`2026-03-02-completion-candidates-quickstart-troubleshooting-validation.md`](./2026-03/02-091100-completion-candidates-quickstart-troubleshooting-validation.md)
- [`2026-03-02-completion-candidates-ci-telemetry-recipe-validation.md`](./2026-03/02-090500-completion-candidates-ci-telemetry-recipe-validation.md)
- [`2026-03-02-completion-candidates-ci-miss-null-ttl-validation.md`](./2026-03/02-090400-completion-candidates-ci-miss-null-ttl-validation.md)
- [`2026-03-02-completion-candidates-ci-miss-hit-only-nullability-validation.md`](./2026-03/02-090300-completion-candidates-ci-miss-hit-only-nullability-validation.md)
- [`2026-03-02-completion-candidates-ci-warm-hit-ttl-consistency-validation.md`](./2026-03/02-090800-completion-candidates-ci-warm-hit-ttl-consistency-validation.md)
- [`2026-03-02-completion-candidates-ci-warm-hit-cache-age-validation.md`](./2026-03/02-090700-completion-candidates-ci-warm-hit-cache-age-validation.md)
- [`2026-03-02-completion-candidates-ci-warm-hit-age-bound-validation.md`](./2026-03/02-090600-completion-candidates-ci-warm-hit-age-bound-validation.md)
- [`2026-03-01-distribution-phase-d-ci-adoption-checkpoint.md`](./2026-03/01-090800-distribution-phase-d-ci-adoption-checkpoint.md)
- [`2026-03-01-distribution-phase-c-homebrew-checkpoint.md`](./2026-03/01-090700-distribution-phase-c-homebrew-checkpoint.md)
- [`2026-03-01-distribution-phase-e-wrapper-evaluation-checkpoint.md`](./2026-03/01-090900-distribution-phase-e-wrapper-evaluation-checkpoint.md)
- [`2026-03-02-distribution-acceptance-closeout-prepublish.md`](./2026-03/02-091200-distribution-acceptance-closeout-prepublish.md)
- [`2026-03-02-distribution-first-publish-runbook-prep.md`](./2026-03/02-091900-distribution-first-publish-runbook-prep.md)
- [`2026-03-02-distribution-first-publish-artifacts-hardening.md`](./2026-03/02-091800-distribution-first-publish-artifacts-hardening.md)
- [`2026-03-02-distribution-closeout-report-generator.md`](./2026-03/02-091700-distribution-closeout-report-generator.md)
- [`2026-03-02-distribution-artifact-validator.md`](./2026-03/02-091600-distribution-artifact-validator.md)
- [`2026-03-02-distribution-artifact-summary-and-auto-validation.md`](./2026-03/02-091500-distribution-artifact-summary-and-auto-validation.md)
- [`2026-03-02-release-checklist-artifact-flow-alignment.md`](./2026-03/02-093000-release-checklist-artifact-flow-alignment.md)
- [`2026-03-02-distribution-artifact-pipeline-smoke.md`](./2026-03/02-091400-distribution-artifact-pipeline-smoke.md)
- [`2026-03-02-distribution-artifact-pipeline-smoke-ci.md`](./2026-03/02-091300-distribution-artifact-pipeline-smoke-ci.md)
- [`2026-03-02-distribution-preflight-command.md`](./2026-03/02-092300-distribution-preflight-command.md)
- [`2026-03-02-distribution-preflight-ci-workflow.md`](./2026-03/02-092200-distribution-preflight-ci-workflow.md)
- [`2026-03-02-distribution-preflight-summary-output.md`](./2026-03/02-092400-distribution-preflight-summary-output.md)
- [`2026-03-02-distribution-first-publish-script-automation.md`](./2026-03/02-092000-distribution-first-publish-script-automation.md)
- [`2026-03-02-distribution-metadata-validation-automation.md`](./2026-03/02-092100-distribution-metadata-validation-automation.md)
- [`2026-03-02-homebrew-metadata-workflow-checkpoint.md`](./2026-03/02-092800-homebrew-metadata-workflow-checkpoint.md)
- [`2026-03-02-homebrew-tap-pr-automation-checkpoint.md`](./2026-03/02-092900-homebrew-tap-pr-automation-checkpoint.md)
- [`2026-03-01-release-gates-automation-checkpoint.md`](./2026-03/01-091300-release-gates-automation-checkpoint.md)
- [`2026-03-01-release-tag-install-validation-checkpoint.md`](./2026-03/01-091400-release-tag-install-validation-checkpoint.md)

- [`2026-03-02-docs-qa-report-index-helper-validation.md`](./2026-03/02-092600-docs-qa-report-index-helper-validation.md)

- [`2026-03-10-script-surface-unification-batch-1.md`](./2026-03/10-090000-script-surface-unification-batch-1.md)

- [`2026-03/05-201451-effigy-northstar-doctrine-alignment.md`](./2026-03/05-201451-effigy-northstar-doctrine-alignment.md)

## Archived Validation Logs

- [`2026-02-26-deferral-fallback-phase-2-1-checkpoint.md`](./2026-02/26-090000-deferral-fallback-phase-2-1-checkpoint.md)
- [`2026-02-26-dev-process-manager-tui-phase-4-checkpoint.md`](./2026-02/26-090100-dev-process-manager-tui-phase-4-checkpoint.md)
- [`2026-02-26-effigy-extraction-and-migration-checkpoint.md`](./2026-02/26-090200-effigy-extraction-and-migration-checkpoint.md)
- [`2026-02-26-legacy-and-active-repo-smoke-sweep.md`](./2026-02/26-090300-legacy-and-active-repo-smoke-sweep.md)
- [`2026-02-26-path-install-and-release-workflow-validation.md`](./2026-02/26-090400-path-install-and-release-workflow-validation.md)
- [`2026-02-27-acowtancy-testing-orchestration-validation.md`](./2026-02/27-090000-acowtancy-testing-orchestration-validation.md)
- [`2026-02-27-builtin-health-fallback-smoke-checkpoint.md`](./2026-02/27-090100-builtin-health-fallback-smoke-checkpoint.md)
- [`2026-02-27-builtin-prefixed-routing-smoke-checkpoint.md`](./2026-02/27-090200-builtin-prefixed-routing-smoke-checkpoint.md)
- [`2026-02-27-catalogs-diagnostics-validation.md`](./2026-02/27-090300-catalogs-diagnostics-validation.md)
- [`2026-02-27-deferral-fallback-closure-validation.md`](./2026-02/27-090400-deferral-fallback-closure-validation.md)
- [`2026-02-27-mixed-repo-suite-selection-validation.md`](./2026-02/27-090500-mixed-repo-suite-selection-validation.md)
- [`2026-02-27-multiprocess-config-consolidation-checkpoint.md`](./2026-02/27-090600-multiprocess-config-consolidation-checkpoint.md)
- [`2026-02-27-runner-and-tui-modularization-checkpoint.md`](./2026-02/27-090700-runner-and-tui-modularization-checkpoint.md)
- [`2026-02-27-symlink-catalog-discovery-fix-checkpoint.md`](./2026-02/27-090800-symlink-catalog-discovery-fix-checkpoint.md)
- [`2026-02-27-tui-core-extraction-checkpoint.md`](./2026-02/27-090900-tui-core-extraction-checkpoint.md)
- [`2026-02-27-tui-modularization-phase-2-checkpoint.md`](./2026-02/27-091000-tui-modularization-phase-2-checkpoint.md)
- [`2026-02-27-tui-modularization-thread.md`](./2026-02/27-091100-tui-modularization-thread.md)
- [`2026-02-27-tui-terminal-emulation-phase-6-1-selection.md`](./2026-02/27-091200-tui-terminal-emulation-phase-6-1-selection.md)
- [`2026-02-27-tui-terminal-emulation-phase-6-1-spike.md`](./2026-02/27-091300-tui-terminal-emulation-phase-6-1-spike.md)
- [`2026-02-28-doctor-roadmap-009-closeout-validation.md`](./2026-02/28-090200-doctor-roadmap-009-closeout-validation.md)
- [`2026-02-28-json-contracts-changed-only-ci.md`](./2026-02/28-090300-json-contracts-changed-only-ci.md)
- [`2026-02-28-json-contracts-ci-policy.md`](./2026-02/28-090400-json-contracts-ci-policy.md)
- [`2026-02-28-json-contracts-selected-schema-logging.md`](./2026-02/28-090500-json-contracts-selected-schema-logging.md)
- [`2026-02-28-json-contracts-selection-artifacts.md`](./2026-02/28-090600-json-contracts-selection-artifacts.md)
- [`2026-02-28-json-contracts-validation.md`](./2026-02/28-090700-json-contracts-validation.md)
- [`2026-02-28-selection-artifact-ci-validation.md`](./2026-02/28-090900-selection-artifact-ci-validation.md)
- [`2026-02-28-selection-artifact-local-validator.md`](./2026-02/28-091000-selection-artifact-local-validator.md)
- [`2026-02-28-selection-payload-contract.md`](./2026-02/28-091100-selection-payload-contract.md)
- [`2026-02-28-selection-validator-negative-smoke.md`](./2026-02/28-091200-selection-validator-negative-smoke.md)
- [`2026-03-01-docs-ia-and-qa-command-consolidation.md`](./2026-03/01-091000-docs-ia-and-qa-command-consolidation.md)
- [`2026-03-01-roadmap-012-batch-12-1-checkpoint.md`](./2026-03/01-091500-roadmap-012-batch-12-1-checkpoint.md)
- [`2026-03-01-roadmap-012-batch-12-2-checkpoint.md`](./2026-03/01-091600-roadmap-012-batch-12-2-checkpoint.md)
- [`2026-03-01-roadmap-012-batch-12-3-checkpoint.md`](./2026-03/01-091700-roadmap-012-batch-12-3-checkpoint.md)
- [`2026-03-01-roadmap-012-batch-12-4-checkpoint.md`](./2026-03/01-091800-roadmap-012-batch-12-4-checkpoint.md)
- [`2026-03-03-watch-friction-sweep.md`](./2026-03/03-090000-watch-friction-sweep.md)
- [`2026-03-04-init-migrate-onboarding-validation.md`](./2026-03/04-090000-init-migrate-onboarding-validation.md)
- [`2026-03-05-doctor-reporting-validation.md`](./2026-03/05-090000-doctor-reporting-validation.md)
- [`2026-03-06-post-m1-regression-pass.md`](./2026-03/06-090000-post-m1-regression-pass.md)
- [`2026-03-07-post-m1-release-readiness-checkpoint.md`](./2026-03/07-090000-post-m1-release-readiness-checkpoint.md)

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
