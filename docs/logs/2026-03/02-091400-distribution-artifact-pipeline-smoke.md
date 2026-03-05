# Distribution Artifact Pipeline Smoke

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmap/backlog/distribution-channels.md`

## Scope

- Add one command that smoke-tests distribution artifact validation and closeout generation together.
- Wire the smoke command into release automation docs.

## Changes

- Added script:
  - `scripts/check-distribution-artifact-pipeline-smoke.sh`
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-artifact-pipeline-smoke.sh && ./scripts/check-distribution-artifact-pipeline-smoke.sh`
  - result: pass

## Outcomes

- Artifact validation and closeout report generation now have a deterministic smoke gate.
- Local/CI troubleshooting can verify the distribution evidence pipeline without a live release tag.

## Risks / Follow-ups

- Smoke uses synthetic artifacts and does not replace real publish-cycle evidence.
- Any future log naming changes must keep smoke fixtures aligned with validator expectations.

## Next Batch Recommendation

- Run first real publish-cycle artifact flow and use generated closeout report to reconcile remaining distribution acceptance criteria.
