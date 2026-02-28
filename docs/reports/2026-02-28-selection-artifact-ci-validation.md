# Selection Artifact CI Validation

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Enforce selection-artifact contract validation in CI before artifact upload.

## Changes

- updated workflow:
  - `.github/workflows/json-contracts.yml`
- extraction behavior:
  - now requires selection JSON line via `grep -m1 '^{"selected":'`
  - no fallback payload is synthesized
- added explicit workflow step:
  - `./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json`
- updated guide:
  - `docs/guides/017-json-output-contracts.md`

## Validation

- command: `GITHUB_EVENT_NAME=pull_request ./scripts/check-json-contracts-ci.sh | tee /tmp/json-contracts.log`
  - result: pass
- command: `grep -m1 '^{"selected":' /tmp/json-contracts.log > /tmp/json-contracts-selected.json`
  - result: pass
- command: `./scripts/validate-json-contract-selection-artifact.sh /tmp/json-contracts-selected.json`
  - result: pass

## Risks / Follow-ups

- workflow now hard-fails when selection JSON is absent; this is intentional to catch regressions in checker output.

## Next

- add a compact contract smoke test in CI that intentionally mutates a temp copy of `json-contracts-selected.json` and verifies validator failure behavior remains intact.
