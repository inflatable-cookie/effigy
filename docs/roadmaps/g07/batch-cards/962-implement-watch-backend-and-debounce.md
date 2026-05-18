# 962 - Implement Watch Backend And Debounce

Roadmap: [`../021-graph-watch-mode-suite.md`](../021-graph-watch-mode-suite.md)
Strict lane: [`../../../specs/088-graph-watch-mode-strict-lane.md`](../../../specs/088-graph-watch-mode-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Add the first working `graph watch` loop.

## Scope

- wire `notify`
- add the `graph watch` CLI surface
- debounce for `1s` by default
- coalesce bursts into one incremental index run
- emit text and JSON watch events

## Guardrails

- no watch-only indexing path
- no hidden background state
- no silent swallowing of backend errors

## Acceptance

- a foreground watch command works in manual proof and tests
- JSON events are typed and versioned
- burst edits result in one bounded refresh cycle

## Next Task

Execute `963`.
