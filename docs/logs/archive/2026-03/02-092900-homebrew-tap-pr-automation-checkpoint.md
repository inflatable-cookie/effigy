# Homebrew Tap PR Automation Checkpoint

Date: 2026-03-02
Owner: Effigy
Related roadmap: `docs/roadmaps/backlog/distribution-channels.md`

## Scope

- Implement tap repository PR automation that consumes `homebrew-metadata-<tag>` artifacts.
- Add a reusable helper script for formula updates from metadata payloads.
- Update distribution/homebrew docs to include workflow wiring and replay path.

## Changes

- Added workflow:
  - `.github/workflows/homebrew-tap-formula-pr.yml`
- Added script:
  - `scripts/update-homebrew-formula-from-metadata.sh`
- Updated guides:
  - `docs/guides/042-homebrew-tap-and-release-automation.md`
  - `docs/guides/044-distribution-first-publish-execution-runbook.md`
  - `docs/guides/024-ci-and-automation-recipes.md`
- Updated backlog roadmap reference:
  - `docs/roadmaps/backlog/distribution-channels.md`

## Validation

- command: `./scripts/check-quality-gates.sh --docs-only && bash -n ./scripts/update-homebrew-formula-from-metadata.sh`
  - result: pass

## Outcomes

- Successful metadata runs can now drive automatic tap PR creation without manual formula editing.
- Manual replay support exists via workflow dispatch with `metadata_run_id` input.
- Formula update logic is centralized and reusable across CI and local verification.

## Risks / Follow-ups

- Automation depends on `EFFIGY_TAP_GH_TOKEN` secret scope in core repo.
- First production run still needs one observed tap PR URL recorded as execution evidence.
- Tap repo CI (`brew audit`, `brew style`, source install smoke) remains the final merge gate.

## Next Batch Recommendation

- Execute the first real release-tag run (`vX.Y.Z`) to capture end-to-end evidence: metadata artifact, tap PR URL, tap CI results, and distribution acceptance-closeout report.
