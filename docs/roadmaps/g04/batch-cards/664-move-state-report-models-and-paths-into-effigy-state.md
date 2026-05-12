# 664 - Move State Report Models And Paths Into Effigy-State

Roadmap: [`../035-state-domain-extraction.md`](../035-state-domain-extraction.md)
Strict lane: [`../../../specs/071-state-domain-extraction-strict-lane.md`](../../../specs/071-state-domain-extraction-strict-lane.md)
Contract: [`../../../contracts/027-state-domain-extraction-contract.md`](../../../contracts/027-state-domain-extraction-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move state report path conventions and history read-model helpers out of the
runner and into `effigy-state`.

## Scope

- move path convention helpers for latest, compatibility, and history report
  locations into `effigy-state`
- move `StateHistoryKind`, history item model, and history report inventory into
  `effigy-state`
- keep filesystem reading/writing behavior equivalent
- keep runner text rendering and command dispatch in `state_command.rs`
- avoid touching unrelated apply-hook execution changes except where type names
  require imports

## Non-Goals

- no apply report model extraction yet
- no capture report model extraction yet
- no hook execution changes
- no state command grammar changes
- no JSON schema changes
- no media/object-store behavior

## Compatibility Boundary

Existing report paths must remain:

- `.effigy/reports/state/<stack>/latest-<kind>.json`
- `.effigy/reports/state/<stack>/<compatibility-name>.json`
- `.effigy/reports/state/<stack>/history/<history-name>.json`

Existing `state history` text and JSON payload shapes must remain compatible.

## Acceptance

- runner no longer owns state report path calculation
- runner no longer owns history report scanning/filtering/classification
- `effigy-state` has focused tests for path and history helpers
- `state history` still renders the same report shape
- unrelated apply-hook worktree changes are preserved

## Outcome

- moved state report path calculation into `effigy-state`
- moved state history scan/filter/classification into `effigy-state`
- added focused `effigy-state` tests for report paths and history summaries
- kept runner ownership for report file writes and text rendering
- preserved the existing apply-hook changes in `state_command.rs`

## Validation

- `cargo test -p effigy-state` passed
- `cargo test state_history` passed
- `cargo check --bin effigy` passed
- `git diff --check` passed

## Next Task

Execute `665` to move pure state plan builders into `effigy-state`.
