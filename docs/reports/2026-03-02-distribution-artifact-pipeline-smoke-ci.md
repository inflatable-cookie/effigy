# Distribution Artifact Pipeline Smoke CI

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmap/backlog/distribution-channels.md`

## Scope

- Add CI workflow coverage for distribution artifact pipeline smoke checks.
- Wire workflow reference into CI automation docs.

## Changes

- Added workflow:
  - `.github/workflows/distribution-artifact-pipeline-smoke.yml`
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-artifact-pipeline-smoke.sh && ruby -e 'require "yaml"; YAML.load_file(".github/workflows/distribution-artifact-pipeline-smoke.yml")' && ./scripts/check-distribution-artifact-pipeline-smoke.sh`
  - result: pass

## Outcomes

- Artifact validation and closeout report generation smoke checks now run automatically on relevant PR/push changes.
- Distribution evidence tooling has continuous CI feedback instead of manual-only verification.

## Risks / Follow-ups

- Workflow is path-filtered; unrelated changes will not trigger this smoke job.
- Real publish-cycle validation is still required for final distribution acceptance closure.

## Next Batch Recommendation

- Execute first real release-tag distribution batch and generate the acceptance closeout report from captured artifacts.
