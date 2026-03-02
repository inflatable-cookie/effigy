# Reports

Reports capture execution evidence, checkpoints, and sweeps.

## Naming convention

Use date-first filenames:

- `YYYY-MM-DD-topic.md`
- `YYYY-MM-DD-HHMM-topic.md` (when multiple same-day reports are needed)

Examples:
- `2026-02-26-effigy-extraction-checkpoint.md`
- `2026-02-26-1545-path-install-smoke.md`

## Thread reports

When a feature spans multiple same-day checkpoints, add a consolidation report that links those checkpoints and provides one final validation matrix.

## Recent Release Notes

- [`../guides/036-release-notes-authoring-template-and-examples.md`](../guides/036-release-notes-authoring-template-and-examples.md) (authoring template + examples)
- [`2026-02-28-dag-watch-onboarding-release-note.md`](./2026-02-28-dag-watch-onboarding-release-note.md)
- [`2026-02-28-json-envelope-removal-release-note.md`](./2026-02-28-json-envelope-removal-release-note.md)
- [`2026-02-28-doctor-explain-mode-release-note.md`](./2026-02-28-doctor-explain-mode-release-note.md)

## Recent Docs IA Reports

- [`2026-03-01-documentation-ia-completion-report.md`](./2026-03-01-documentation-ia-completion-report.md)

## Recent Validation Reports

- [`2026-03-01-effigy-caching-phase-1-validation.md`](./2026-03-01-effigy-caching-phase-1-validation.md)
- [`2026-03-01-shell-completion-and-command-discovery-validation.md`](./2026-03-01-shell-completion-and-command-discovery-validation.md)
- [`2026-03-01-completion-candidates-phase-2-validation.md`](./2026-03-01-completion-candidates-phase-2-validation.md)
- [`2026-03-01-completion-candidates-memoization-validation.md`](./2026-03-01-completion-candidates-memoization-validation.md)
- [`2026-03-01-completion-candidates-cache-state-validation.md`](./2026-03-01-completion-candidates-cache-state-validation.md)
- [`2026-03-01-completion-candidates-cache-age-validation.md`](./2026-03-01-completion-candidates-cache-age-validation.md)
- [`2026-03-01-completion-candidates-cache-ttl-validation.md`](./2026-03-01-completion-candidates-cache-ttl-validation.md)
- [`2026-03-01-completion-candidates-cache-manifest-digest-validation.md`](./2026-03-01-completion-candidates-cache-manifest-digest-validation.md)
- [`2026-03-01-completion-candidates-cache-ttl-override-validation.md`](./2026-03-01-completion-candidates-cache-ttl-override-validation.md)
- [`2026-03-02-completion-candidates-cache-policy-telemetry-validation.md`](./2026-03-02-completion-candidates-cache-policy-telemetry-validation.md)
- [`2026-03-02-completion-candidates-cache-policy-env-invalid-validation.md`](./2026-03-02-completion-candidates-cache-policy-env-invalid-validation.md)
- [`2026-03-02-completion-candidates-cache-policy-json-contract-validation.md`](./2026-03-02-completion-candidates-cache-policy-json-contract-validation.md)
- [`2026-03-02-completion-candidates-json-contract-docs-validation.md`](./2026-03-02-completion-candidates-json-contract-docs-validation.md)
- [`2026-03-02-completion-candidates-quickstart-troubleshooting-validation.md`](./2026-03-02-completion-candidates-quickstart-troubleshooting-validation.md)
- [`2026-03-02-completion-candidates-ci-telemetry-recipe-validation.md`](./2026-03-02-completion-candidates-ci-telemetry-recipe-validation.md)
- [`2026-03-02-completion-candidates-ci-miss-null-ttl-validation.md`](./2026-03-02-completion-candidates-ci-miss-null-ttl-validation.md)
- [`2026-03-02-completion-candidates-ci-warm-hit-ttl-consistency-validation.md`](./2026-03-02-completion-candidates-ci-warm-hit-ttl-consistency-validation.md)
- [`2026-03-01-distribution-phase-d-ci-adoption-checkpoint.md`](./2026-03-01-distribution-phase-d-ci-adoption-checkpoint.md)
- [`2026-03-01-distribution-phase-c-homebrew-checkpoint.md`](./2026-03-01-distribution-phase-c-homebrew-checkpoint.md)
- [`2026-03-01-distribution-phase-e-wrapper-evaluation-checkpoint.md`](./2026-03-01-distribution-phase-e-wrapper-evaluation-checkpoint.md)
- [`2026-03-02-distribution-acceptance-closeout-prepublish.md`](./2026-03-02-distribution-acceptance-closeout-prepublish.md)
- [`2026-03-02-distribution-first-publish-runbook-prep.md`](./2026-03-02-distribution-first-publish-runbook-prep.md)
- [`2026-03-02-distribution-first-publish-artifacts-hardening.md`](./2026-03-02-distribution-first-publish-artifacts-hardening.md)
- [`2026-03-02-distribution-closeout-report-generator.md`](./2026-03-02-distribution-closeout-report-generator.md)
- [`2026-03-02-distribution-artifact-validator.md`](./2026-03-02-distribution-artifact-validator.md)
- [`2026-03-02-distribution-artifact-summary-and-auto-validation.md`](./2026-03-02-distribution-artifact-summary-and-auto-validation.md)
- [`2026-03-02-release-checklist-artifact-flow-alignment.md`](./2026-03-02-release-checklist-artifact-flow-alignment.md)
- [`2026-03-02-distribution-first-publish-script-automation.md`](./2026-03-02-distribution-first-publish-script-automation.md)
- [`2026-03-02-distribution-metadata-validation-automation.md`](./2026-03-02-distribution-metadata-validation-automation.md)
- [`2026-03-02-homebrew-metadata-workflow-checkpoint.md`](./2026-03-02-homebrew-metadata-workflow-checkpoint.md)
- [`2026-03-02-homebrew-tap-pr-automation-checkpoint.md`](./2026-03-02-homebrew-tap-pr-automation-checkpoint.md)
- [`2026-03-01-release-gates-automation-checkpoint.md`](./2026-03-01-release-gates-automation-checkpoint.md)
- [`2026-03-01-release-tag-install-validation-checkpoint.md`](./2026-03-01-release-tag-install-validation-checkpoint.md)

## Report template

```md
# <Report Title>

Date: YYYY-MM-DD
Owner: <team/person>
Related roadmap: <id/title>

## Scope
- ...

## Changes
- ...

## Validation
- command: `...`
  - result: ...

## Risks / Follow-ups
- ...

## Next
- ...
```
