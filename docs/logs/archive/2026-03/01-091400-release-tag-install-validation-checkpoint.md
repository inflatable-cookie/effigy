# Release Tag Install Validation Checkpoint

Date: 2026-03-01
Owner: Platform
Related roadmap: backlog/distribution-channels (Phase B)

## Scope
- Add automated validation for `cargo install --git --tag` as part of release gates.
- Wire tag/install checks into release CI workflow and release docs/checklists.

## Changes
- Added `scripts/check-release-install-from-tag.sh`.
- Extended `scripts/check-release-gates.sh` with optional `--tag` and `--repo-url` arguments and tag-install validation step.
- Updated `.github/workflows/release-gates.yml` to pass tag/repo context on tag pushes and manual dispatch.
- Updated docs for release/install workflow and backlog tracking:
  - `README.md`
  - `docs/guides/010-path-installation-and-release.md`
  - `docs/guides/014-release-checklist-template.md`
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/roadmaps/backlog/release-contract-v0.md`
  - `docs/roadmaps/backlog/distribution-channels.md`

## Validation
- command: `./scripts/check-release-gates.sh`
  - result: pass (format, full tests, quality gates, release build, release smoke); tag-install step intentionally skipped because no `--tag` was provided in local run.

## Risks / Follow-ups
- Local validation cannot assert live tag-install behavior without a published/pushed tag reference.
- `cargo install` from crates.io remains pending until first crates publish cycle.

## Next Batch Recommendation
- Batch D2: add CI pinned-install snippets and migration guidance docs for teams using wrapper scripts, then run one consolidated validation and publish a distribution adoption checkpoint.
