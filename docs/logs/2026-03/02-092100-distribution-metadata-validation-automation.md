# Distribution Metadata Validation Automation

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/release-contract-v0.md`

## Scope

- Add an automated distribution metadata validator for release readiness.
- Integrate metadata checks into consolidated release gates.
- Update release contract/checklist/docs to reference the new validator.

## Changes

- Added script:
  - `scripts/check-distribution-metadata.sh`
- Updated release gates:
  - `scripts/check-release-gates.sh` (now runs metadata validation)
- Updated docs:
  - `docs/roadmaps/backlog/release-contract-v0.md`
  - `docs/guides/014-release-checklist-template.md`
  - `docs/guides/024-ci-and-automation-recipes.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/check-distribution-metadata.sh ./scripts/check-release-gates.sh && ./scripts/check-distribution-metadata.sh`
  - result: pass

## Outcomes

- Release gates now validate crate metadata, install/release guide presence, and distribution workflow wiring in one automated path.
- The release contract checklist item for distribution metadata is now executable and no longer manual-only.

## Risks / Follow-ups

- First release-tag run still needs publish-cycle evidence (`--tag vX.Y.Z`) to prove version/tag alignment in CI.
- Validator enforces file/wiring presence; it does not verify external systems (crates.io publication success, tap CI health).

## Next Batch Recommendation

- Execute first real publish-cycle runbook (`./scripts/check-distribution-first-publish.sh --tag vX.Y.Z`) and publish acceptance-closeout evidence across tag install, crates.io install, and Homebrew upgrade path.
