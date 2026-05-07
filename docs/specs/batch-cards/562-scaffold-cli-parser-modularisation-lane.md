# 562 - Scaffold CLI Parser Modularisation Lane

Lane: [`051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md`](../051-cli-parser-modularisation-for-runtime-surfaces-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-07

## Goal

Inventory the current parser hotspots and select the first bounded `g04.009`
implementation slice.

## Scope

- measure current parser file sizes
- inventory parse tests for container, artifact, and bootstrap surfaces
- identify the safest first parser split
- update the lane with the chosen first implementation card
- no parser code movement yet

## Non-Goals

- no public CLI behavior changes
- no parser rewrites
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the parser hotspot inventory is recorded and the
first implementation card is ready.

## Validation

- parser file line-count scan
- parse-test inventory scan
- `git diff --check`

## Next Task

Scaffold CLI parser modularisation lane.
