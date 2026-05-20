# 1026 - Build Cross-Repo Agent Usage Benchmark

Roadmap: [`../076-cross-repo-agent-usage-benchmark.md`](../076-cross-repo-agent-usage-benchmark.md)
Strict lane: [`../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-20

## Purpose

Measure whether graph reduces agent navigation work on real tasks.

## Work

- define benchmark cases and expected owner/test outcomes
- support fixture-backed cases in this repo
- support optional live cases for Underlay and decodelabs repos
- emit human-readable and JSON output
- record graph command count, fallback search count, first-hit correctness, and
  enough timing data to spot large regressions

## Guardrails

- optional local repos must skip cleanly when absent
- no inflated percentage claims
- no benchmark that only works on Effigy
- no CI dependency on private local repos

## Acceptance

- benchmark runs with fixtures on any machine
- optional live repo cases are documented
- output can support closeout without manual scraping

## Next Task

Move to `1027`.
