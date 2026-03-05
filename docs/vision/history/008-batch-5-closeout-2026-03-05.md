# 008 Batch 5 Closeout - 2026-03-05

Status: Complete
Date: 2026-03-05
Purpose: close Batch 5 by formalizing CI layout conventions and enforcing docs-referenced workflow path validity.

## 1. Scope Completed

1. Added dedicated workflow path validator:
- `docs/scripts/check-doc-workflow-paths.sh`

2. Integrated workflow path validation into vision/docs metadata gate:
- `docs/scripts/check-vision-metadata.sh` now runs `docs/scripts/check-doc-workflow-paths.sh`

3. Updated CI/workflow references in non-report docs to current repository layout (`.github-bak/workflows/*.yml`):
- `docs/guides/024-ci-and-automation-recipes.md`
- `docs/guides/035-guide-ownership-and-update-triggers.md`
- `docs/guides/036-release-notes-authoring-template-and-examples.md`
- `docs/guides/042-homebrew-tap-and-release-automation.md`
- `docs/guides/044-distribution-first-publish-execution-runbook.md`
- `docs/roadmaps/backlog/distribution-channels.md`
- `docs/roadmaps/backlog/release-contract-v0.md`

4. Documented CI layout conventions and enforcement commands in docs process guides:
- `docs/guides/029-docs-qa-checklist-and-validation.md`
- `docs/guides/037-documentation-contribution-playbook.md`
- `docs/guides/039-docs-drift-monitoring.md`

## 2. Compliance Results

| Artifact Group | Requirement | Result |
| --- | --- | --- |
| Docs QA checks | docs-referenced workflow paths validated | Pass |
| CI layout policy | `.github-bak` vs `.github/workflows` convention documented | Pass |
| Docs references | active non-report references point to existing workflow files | Pass |

## 3. Validation

- command: `./docs/scripts/check-doc-workflow-paths.sh`
  - result: pass
- command: `./docs/scripts/check-vision-metadata.sh`
  - result: pass
- command: `cargo qa-docs`
  - result: pass

## 4. Residual Gaps

1. Historical reports in `docs/logs/` intentionally retain original path references where they reflect historical context.
2. `cargo qa-docs` does not yet execute `check-doc-workflow-paths.sh` directly; enforcement currently occurs via `check-vision-metadata.sh`.

## 5. Decision

Batch 5 is accepted as complete for docs policy + path-validation scope.

## Next Task

Execute Batch 6: add `docs/scripts/check-doc-workflow-paths.sh` directly to the docs-only quality gate runner and document the historical-report exception policy explicitly in report-authoring guidance.
