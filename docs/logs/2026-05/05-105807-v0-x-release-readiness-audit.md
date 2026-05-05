# 2026-05-05 - v0.x Release Readiness Audit

## Summary

Completed card `373` against the live `v0.x` release contract.

The repo is ready at the non-destructive release-status layer:

- current version: `0.3.3`
- suggested next version: `0.4.0`
- suggested tag: `v0.4.0`
- changelog valid: yes
- blockers: none
- unreleased entries: 18
- configured gates: 6
- gates run: no

## Checks

- `cargo run --quiet --bin effigy -- release status --json`
- `cargo run --quiet --bin effigy -- release --help`
- `cargo run --quiet --bin effigy -- tasks --json`
- `cargo run --quiet --bin effigy -- release prepare --plan --json`
- `cargo run --quiet --bin effigy -- changelog extract CHANGELOG.md --version Unreleased`
- `cargo run --quiet --bin effigy -- distribution --help`
- `cargo run --quiet --bin effigy -- distribution validate-metadata --json`
- `cargo run --quiet --bin effigy -- docs check-links docs/roadmaps/backlog/release-contract-v0.md docs/guides/014-release-checklist-template.md docs/guides/062-distribution-system-guide.md`
- `git diff --check`

## Findings

- `release status --json` reports `ready: true` with no blockers.
- `release prepare --plan --json` previews `0.4.0`, `v0.4.0`, changelog
  promotion, and `Cargo.lock` sync without writing state.
- the changelog has release-note source material for the recent user-facing
  prompt guardrail work.
- release gates are configured in `release/effigy.release.toml`, but were not
  run during this audit.
- the distribution metadata validator passes.
- docs had small drift from the current distribution surface.

## Repairs

- updated the live `v0.x` release contract to name GitHub Releases, Homebrew,
  and tagged source install instead of the old crates/Homebrew wording.
- replaced removed distribution helper-script references with the current
  `effigy distribution ...` built-ins.
- changed the release checklist channel heading from `Crates` to `GitHub
  Releases and Source Install`.
- removed a stale `--tag` argument from the `distribution validate-artifacts`
  cycle example.

## Boundary

No release prepare/apply, execute, tag, push, or workflow edit was run.

## Next

No active ready card. A human can request the next release flow explicitly, or
the repo can stay in planning.
