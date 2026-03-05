# Distribution Preflight Command

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Add one non-publish preflight command for distribution readiness.
- Wire preflight usage into release and distribution runbook docs.

## Changes

- Added script:
  - `scripts/check-distribution-preflight.sh`
- Updated docs:
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`
  - `docs/guides/014-release-checklist-template.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-preflight.sh && ./scripts/check-distribution-preflight.sh --tag v0.1.0`
  - result: pass

## Outcomes

- Distribution readiness now has a single preflight entrypoint that runs docs gate, metadata validation, and artifact-pipeline smoke checks.
- Release operators can detect tooling/config drift before running real publish-cycle commands.

## Risks / Follow-ups

- Preflight intentionally avoids real publish actions; it does not replace release-tag and crates.io/Homebrew execution evidence.
- Script dependencies remain local-tool dependent (`jq`, shell tooling).

## Next Batch Recommendation

- Execute first real release-tag artifact flow and close acceptance criteria with generated closeout evidence.
