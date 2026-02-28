# JSON Contracts Selection Artifacts

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Add machine-readable schema-selection output and persist it as CI artifacts for review/debugging.

## Changes

- added `--print-selected=json` mode to `scripts/check-json-contracts.sh`
- updated CI helper `scripts/check-json-contracts-ci.sh` to use JSON selection mode in all paths
- updated workflow `.github/workflows/json-contracts.yml`:
  - captures full log to `json-contracts.log`
  - extracts first selection JSON line to `json-contracts-selected.json`
  - uploads both files via `actions/upload-artifact@v4`
- updated guide `docs/guides/017-json-output-contracts.md` with JSON selection mode and artifact behavior

## Validation

- command: `./scripts/check-json-contracts.sh --fast --print-selected=json`
  - result: pass, emits compact selection JSON line
- command: `./scripts/check-json-contracts.sh --fast --changed-only HEAD --print-selected=json`
  - result: pass, emits `selected=[]` JSON summary
- command: `GITHUB_EVENT_NAME=pull_request ./scripts/check-json-contracts-ci.sh`
  - result: pass, emits selection JSON line before checks

## Risks / Follow-ups

- artifact extraction uses `grep` against first `{"selected":...}` line; if output format changes, extraction should be updated accordingly.

## Next

- add schema-selection JSON contract doc (fields and meaning) under `docs/contracts/` so downstream tooling can depend on it explicitly.
