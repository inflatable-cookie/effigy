# 963 - Add Overflow Reconcile And Surface Proof

Roadmap: [`../021-graph-watch-mode-suite.md`](../021-graph-watch-mode-suite.md)
Strict lane: [`../../../specs/088-graph-watch-mode-strict-lane.md`](../../../specs/088-graph-watch-mode-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make watch mode reliable when the event stream is incomplete or noisy.

## Scope

- add explicit dirty/overflow event states
- add reconcile fallback
- prove delete and burst behavior after fallback

## Guardrails

- no fake success on overflow
- no stale graph after reconcile

## Acceptance

- dirty fallback is explicit in output
- reconcile returns the graph to clean state
- tests and manual proof cover the fallback path

## Next Task

Execute `964`.
