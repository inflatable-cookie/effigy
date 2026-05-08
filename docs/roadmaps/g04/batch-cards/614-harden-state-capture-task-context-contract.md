# 614 - Harden State Capture Task Context Contract

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Make the repo-owned capture task context stable enough for Acowtancy to build
against without scraping ad hoc environment variables.

## Scope

- document the capture task context contract
- decide whether the first stable context is environment-only or also written as
  a JSON context file
- expose parent lineage, capture role, mode, source environment, key, source,
  and destination ref consistently
- add focused tests for the emitted context
- keep app-specific capture payload generation inside the repo task

## Non-Goals

- no Acowtancy transform implementation
- no record-level conflict detection
- no automatic old-site sync
- no release work

## Exit Condition

This card is complete when capture tasks have a documented and tested stable
context surface that an app repo can consume as the first Acowtancy proof seam.

## Validation

- focused state capture CLI tests
- JSON contract checks if payload shape changes
- docs path checks for touched docs
- `git diff --check`

## Next Task

Run the first Acowtancy-side rebase proof against the Effigy state-stack
surface.
