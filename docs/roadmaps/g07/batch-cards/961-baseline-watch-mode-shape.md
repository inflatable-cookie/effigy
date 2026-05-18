# 961 - Baseline Watch Mode Shape

Roadmap: [`../021-graph-watch-mode-suite.md`](../021-graph-watch-mode-suite.md)
Strict lane: [`../../../specs/088-graph-watch-mode-strict-lane.md`](../../../specs/088-graph-watch-mode-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Lock the first watch-mode contract before backend code lands.

## Scope

- pin the command shape for `graph watch`
- pin the default debounce
- pin text and JSON event families
- pin the first dirty/overflow fallback posture

## Guardrails

- no implementation drift in this card
- no daemon creep
- no detached service lifecycle

## Acceptance

- one checkpoint log records the watch-mode contract
- `962` and `963` have concrete implementation targets

## Next Task

Execute `962`.
