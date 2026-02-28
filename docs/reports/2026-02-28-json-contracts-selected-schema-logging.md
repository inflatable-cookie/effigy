# JSON Contracts Selected Schema Logging

Date: 2026-02-28
Owner: Platform
Related roadmap: Backlog - JSON output contracts

## Scope

Improve JSON contract CI observability by printing the selected schema ids before validation runs.

## Changes

- added `--print-selected` option to `scripts/check-json-contracts.sh`
- selected schema output now prints as:
  - `[selected] <schema-id>`
  - `[selected] none (...)` when no schema rows are selected
- updated `scripts/check-json-contracts-ci.sh` to always pass `--print-selected`
  - PR changed-only path
  - PR fallback fast path
  - non-PR full path
- documented option and CI behavior in `docs/guides/017-json-output-contracts.md`

## Validation

- command: `./scripts/check-json-contracts.sh --fast --print-selected`
  - result: pass with selected schema ids printed
- command: `./scripts/check-json-contracts.sh --fast --changed-only HEAD --print-selected`
  - result: pass with `selected none` output
- command: `GITHUB_EVENT_NAME=pull_request ./scripts/check-json-contracts-ci.sh`
  - result: pass with selected schema ids printed

## Risks / Follow-ups

- selected schema logging is line-based; if future output formatting changes, CI parsing should continue to treat these as informational only.

## Next

- add `--print-selected=json` mode for machine-readable selection diagnostics in CI artifacts.
