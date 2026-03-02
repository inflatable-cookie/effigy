# Distribution Preflight CI Workflow

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmap/backlog/distribution-channels.md`

## Scope

- Add CI coverage for distribution preflight readiness checks.
- Link workflow entrypoint in automation guide.

## Changes

- Added workflow:
  - `.github/workflows/distribution-preflight.yml`
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-preflight.sh && ruby -e 'require "yaml"; YAML.load_file(".github/workflows/distribution-preflight.yml")' && ./scripts/check-distribution-preflight.sh --tag v0.1.0`
  - result: pass

## Outcomes

- Distribution readiness now has automated CI coverage through a single preflight workflow.
- Drift in metadata/docs/artifact pipeline wiring is caught before publish-window operations.

## Risks / Follow-ups

- Workflow uses `v0.1.0` tag as a preflight alignment value; this should be updated if project version changes.
- Real publish-cycle evidence remains required for acceptance closure.

## Next Batch Recommendation

- Run first real release-tag matrix and publish acceptance closeout report generated from captured artifacts.
