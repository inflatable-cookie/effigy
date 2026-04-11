# 008 Decide Demo Runner Lifecycle And Artifact Boundaries

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Lock the first bounded runner contract for demos:

- discovery and selection expectations
- run / stop / rerun lifecycle
- minimal status model
- artifact and receipt boundary

## In Scope

- define what the runner owns versus what the later TUI owns
- define the minimum lifecycle and state model
- define how runnable entrypoints attach to demo declarations
- define minimum artifact/receipt semantics without overdesigning rendering

## Out Of Scope

- TUI implementation
- desktop-client decisions
- migration of Signal or other repos
- rich artifact viewers or visual diff tooling

## Acceptance Criteria

- `g02.003` clearly states the demo runner lifecycle contract
- artifact and receipt handling are explicit enough for later clients
- the next batch can move to coverage/gap modeling or browser contract without
  reopening lifecycle basics

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch drifts into TUI layout or desktop runtime decisions
- artifact semantics turn into a bespoke file-format design exercise

## Next Task

Complete this planning batch, then leave the next move explicit as either
coverage/gap modeling or the first browser/TUI contract batch.
