# Release Gates Automation Checkpoint

Date: 2026-03-01
Owner: Platform
Related roadmap: backlog/distribution-channels (Phase B readiness)

## Scope
- Add one-command release gate automation for local and CI tag flows.
- Codify release smoke checks that include prefixed built-in invocations.
- Update release/distribution docs to reflect new gate entrypoints.

## Changes
- Added release gate script: `scripts/check-release-gates.sh`.
- Added release smoke script: `scripts/check-release-smoke.sh`.
- Added cargo alias and launcher binary:
  - `.cargo/config.toml` -> `cargo qa-release`
  - `src/bin/effigy-release-qa.rs`
- Added tag-driven CI workflow: `.github/workflows/release-gates.yml`.
- Updated release guidance and automation docs:
  - `README.md`
  - `docs/guides/010-path-installation-and-release.md`
  - `docs/guides/014-release-checklist-template.md`
  - `docs/guides/024-ci-and-automation-recipes.md`
  - `docs/roadmap/backlog/release-contract-v0.md`
  - `docs/roadmap/backlog/distribution-channels.md`
  - `docs/reports/README.md`

## Validation
- command: `cargo qa-release`
  - result: pass (`fmt`, `cargo test`, docs links, JSON contract checks, release build, release smoke checks)

## Risks / Follow-ups
- `cargo install` from tag/crates.io validation is not automated yet; currently still a manual checklist item.
- Homebrew tap/formula automation remains out of scope for this batch.

## Next Batch Recommendation
- Batch D1 (Distribution): add a deterministic install-validation script for `cargo install --git <repo> --tag <tag>` and integrate it into release gates (manual dispatch + tag metadata input), with docs updates in `010`, `014`, and backlog Phase B acceptance notes.
