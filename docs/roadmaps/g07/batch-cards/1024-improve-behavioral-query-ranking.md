# 1024 - Improve Behavioral Query Ranking

Roadmap: [`../074-behavioral-query-ranking-and-vocabulary.md`](../074-behavioral-query-ranking-and-vocabulary.md)
Strict lane: [`../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-20

## Purpose

Reduce query phrasing sensitivity for behavior-shaped questions.

## Work

- inspect tokenization, role inference, source evidence, and path scoring
- add generic behavior vocabulary support
- add gold queries for prompt/shutdown, validation, redirect, migration, and
  cache/index behaviors
- verify result reasons explain any boosts
- compare against exact-token search cases to avoid regressions

## Guardrails

- no hard-coded Effigy module names
- no LLM dependency
- no broad fuzzy matching that floods results
- every ranking boost must be explainable

## Acceptance

- the shell-exit prompt query lands on the correct owner with natural wording
- non-Effigy behavioral fixtures improve or stay stable
- exact-token lookups do not regress

## Next Task

Move to `1025`.
