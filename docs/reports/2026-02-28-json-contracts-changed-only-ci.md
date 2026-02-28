# JSON Contracts Changed-Only CI Integration

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Integrate changed-only schema validation into CI pull request runs with safe fallback behavior.

## Changes

- added CI helper script: `scripts/check-json-contracts-ci.sh`
- updated workflow to call helper for all events:
  - `.github/workflows/json-contracts.yml`
- PR behavior in helper:
  - fetches PR base ref (`origin/$GITHUB_BASE_REF`)
  - resolves fetched base commit (`FETCH_HEAD`)
  - runs `./scripts/check-json-contracts.sh --fast --changed-only <base-commit>`
  - falls back to `./scripts/check-json-contracts.sh --fast` when base resolution fails
- non-PR behavior in helper:
  - runs full check `./scripts/check-json-contracts.sh`
- updated guide policy section in `docs/guides/017-json-output-contracts.md`

## Validation

- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast --changed-only HEAD`
  - result: pass (`no changed active schema entries`)
- command: `GITHUB_EVENT_NAME=pull_request GITHUB_BASE_REF=main ./scripts/check-json-contracts-ci.sh`
  - result: pass (changed-only path executed)
- command: `GITHUB_EVENT_NAME=pull_request ./scripts/check-json-contracts-ci.sh`
  - result: pass (fallback fast path executed)

## Risks / Follow-ups

- PR base-ref fetch assumes standard `origin` remote naming in CI.
- changed-only diffing is scoped to `docs/contracts/json-schema-index.json`; future schema source expansion may require broader selection rules.

## Next

- add a small `--print-selected` mode to `check-json-contracts.sh` to expose selected schema ids in CI logs for easier review/debugging.
