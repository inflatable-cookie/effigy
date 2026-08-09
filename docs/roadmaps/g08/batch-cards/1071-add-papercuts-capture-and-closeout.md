# 1071 - Add Papercuts Capture And Closeout

Roadmap: [`../027-papercuts-discovery-and-capture.md`](../027-papercuts-discovery-and-capture.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/036-papercuts-discovery-contract.md`](../../../contracts/036-papercuts-discovery-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-09
Ready after: card 1070

## Purpose

Add safe canonical capture for one project, publish the operator/agent surface,
and close the strict lane with full evidence.

## Work

- add required-field grammar for `papercuts add`
- create missing queues from the canonical compatible starter
- insert newest entries first while preserving unrelated Markdown
- reject collection targets and exact normalized open-title duplicates
- use locked atomic replacement
- update command, JSON, agent, and changelog guidance
- run focused and repository-wide validation; close all currentness surfaces

## Acceptance

- [x] add works in one existing or new project queue
- [x] duplicate open titles and collection scope fail without mutation
- [x] concurrent/failed writes cannot leave a partial queue
- [x] discovery immediately reads the inserted canonical entry
- [x] docs and JSON contracts describe the shipped surface
- [x] strict lane and evidence log close together

## Validation

- focused domain and CLI tests
- add round-trip and no-mutation failure tests
- `effigy qa:ci:fast`
- `effigy qa:docs`
- `effigy qa:json`
- `git diff --check`

## Stop Conditions

Stop before semantic duplicate decisions, close/promotion behavior, workflow
edits, release operations, or modification outside the selected project queue.

## Next Task

Run the first periodic portfolio triage from `~/Dev/projects`.
