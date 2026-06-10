# Selection Validator Negative Smoke

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Add a CI smoke guard that asserts the selection-artifact validator continues to reject invalid payloads.

## Changes

- added smoke script:
  - `scripts/check-selection-artifact-validator-smoke.sh`
- script behavior:
  - generates a valid fixture and requires validator success
  - generates invalid fixtures and requires validator failure:
    - `count` mismatch
    - invalid `mode` enum (`unknown`)
    - non-string item in `selected`
- updated workflow:
  - `.github/workflows/json-contracts.yml`
  - added step: `Validator smoke check (negative path)`
- updated guide:
  - `docs/guides/017-json-output-contracts.md`

## Validation

- command: `./scripts/check-selection-artifact-validator-smoke.sh`
  - result: pass
- command: `set -o pipefail; GITHUB_EVENT_NAME=pull_request ./scripts/check-json-contracts-ci.sh | tee /tmp/json-contracts.log >/dev/null; grep -m1 '^{"selected":' /tmp/json-contracts.log > /tmp/json-contracts-selected.json; ./scripts/validate-json-contract-selection-artifact.sh /tmp/json-contracts-selected.json; ./scripts/check-selection-artifact-validator-smoke.sh`
  - result: pass

## Risks / Follow-ups

- smoke fixtures are intentionally minimal; they validate core invariants but not every possible malformed shape.
