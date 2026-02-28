# JSON Contracts CI Policy

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Define an automated CI policy for JSON schema contract validation so machine-consumable output surfaces remain stable.

## Changes

- added GitHub Actions workflow: `.github/workflows/json-contracts.yml`
- configured event policy:
  - pull request: fast contract check (`--fast`)
  - push to `main`: full contract check
  - nightly schedule (`02:00 UTC`): full contract check
  - manual trigger (`workflow_dispatch`): full contract check
- documented policy in `docs/guides/017-json-output-contracts.md`

## Validation

- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast --changed-only HEAD`
  - result: pass (`no changed active schema entries`)

## Risks / Follow-ups

- `--changed-only` is currently intended for local/targeted usage and not yet wired into CI path-specific filtering.
- full checks include `effigy --json test` and can take longer depending on workspace and runner.

## Next

- integrate path-aware CI optimization (`--changed-only <base-ref>`) for PRs as an optional optimization after runtime baselines are captured.
