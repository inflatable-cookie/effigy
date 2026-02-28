# Selection Artifact Local Validator

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Add a one-command local replay validator for `json-contracts-selected.json` artifacts produced by JSON-contract CI.

## Changes

- added validator script:
  - `scripts/validate-json-contract-selection-artifact.sh`
- validator behavior:
  - loads contract at `docs/contracts/json-selection-contract.json`
  - validates artifact JSON syntax
  - validates required keys, field types, mode enum, and invariants:
    - `count == len(selected)`
    - `selected[*]` are strings
- added guide usage in `docs/guides/017-json-output-contracts.md`

## Validation

- command: `./scripts/check-json-contracts.sh --fast --print-selected=json > /tmp/json-contracts-run.log && sed -n '1p' /tmp/json-contracts-run.log > /tmp/json-contracts-selected.json`
  - result: pass (artifact fixture created)
- command: `./scripts/validate-json-contract-selection-artifact.sh /tmp/json-contracts-selected.json`
  - result: pass
- command: `printf '{\"selected\":[],\"count\":1,\"changed_only_base\":null,\"mode\":\"fast\"}\n' >/tmp/json-contracts-selected-invalid.json`
  - result: fixture created
- command: `./scripts/validate-json-contract-selection-artifact.sh /tmp/json-contracts-selected-invalid.json`
  - result: fail (as expected; count invariant violated)

## Risks / Follow-ups

- validator currently enforces contract semantics directly (via jq) rather than generic JSON-Schema runtime validation.

## Next

- add an optional CI post-step that validates generated `json-contracts-selected.json` with this script before artifact upload.
