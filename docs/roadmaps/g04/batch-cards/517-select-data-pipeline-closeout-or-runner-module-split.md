# 517 - Select Data Pipeline Closeout or Runner Module Split

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Decide whether `g04.005` is ready to close or needs one more runner module
split before handoff to Rhai host cleanup.

## Scope

- review remaining ownership in `src/runner/db_seed.rs`
- review remaining ownership in `src/runner/container_command/data.rs`
- compare current state to `g04.005` acceptance criteria
- choose either closeout or one bounded module-split card
- update the lane and roadmap front doors with the selected next card

## Non-Goals

- no code movement unless the review finds a tiny mechanical docs/front-door fix
- no new data feature work
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the next `g04.005` move is explicit: close the
milestone, or split one concrete runner-owned module surface.

## Closeout

Selected one bounded runner module split before closeout. The remaining
high-value cut was container data prompt policy and rendering because it was
still mixed into `src/runner/container_command/data.rs` with dump/seed
orchestration.

## Validation

- docs/front-door consistency check passed during card selection

## Next Task

Start card
[`518-split-container-data-prompt-module.md`](./518-split-container-data-prompt-module.md).
