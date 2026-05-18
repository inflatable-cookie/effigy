# 931 - Baseline Incremental, Query, And Failed-Path Inventory

Roadmap: [`../013-graph-follow-up-performance-and-fixture-reliability.md`](../013-graph-follow-up-performance-and-fixture-reliability.md)
Strict lane: [`../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md`](../../../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Reconfirm the measured follow-up target before implementation changes start.

## Scope

- restate the `g07.012` cold/no-op/query baselines
- inventory the seven failed full-repo paths by failure class
- identify the first profitable incremental-index seam
- identify the first profitable query hot path

## Guardrails

- no implementation drift in this card
- no redefining the baseline after fixes land
- no hidden exclusion of failed paths

## Acceptance

- follow-up metrics are recorded in one checkpoint log
- `932`, `933`, and `934` have concrete targets

## Next Task

Execute `932`.
