# Selection Payload Contract

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Formalize the JSON payload contract emitted by `--print-selected=json` and enforce payload shape in the checker.

## Changes

- added formal contract document:
  - `docs/contracts/json-selection-contract.json`
- enforced payload contract assertion in:
  - `scripts/check-json-contracts.sh`
- updated guide reference in:
  - `docs/guides/017-json-output-contracts.md`

## Validation

- command: `./scripts/check-json-contracts.sh --fast --print-selected=json`
  - result: pass
- command: `./scripts/check-json-contracts.sh --full --changed-only HEAD --print-selected=json`
  - result: pass (no changed active schema entries path)
- command: `GITHUB_EVENT_NAME=push ./scripts/check-json-contracts-ci.sh`
  - result: pass (selection payload emitted and validated before checks)

## Risks / Follow-ups

- contract assertions currently run at emit-time only; future refactors should keep these checks close to payload construction.

## Next

- add a tiny parser utility under `scripts/` that validates `json-contracts-selected.json` artifact files against `docs/contracts/json-selection-contract.json` for local CI replay.
