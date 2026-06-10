# 997 - Reduce Warm Query Latency And Fix Release Ranking

Roadmap: [`../047-warm-query-latency-and-release-ranking.md`](../047-warm-query-latency-and-release-ranking.md)
Strict lane: [`../../../specs/092-codegraph-parity-follow-up-strict-lane.md`](../../../specs/092-codegraph-parity-follow-up-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Fix the warm-query regression that currently blocks honest parity claims.

## Work

- profile and reduce hot-path warm-query cost
- preserve current owner quality
- retest the active live-repo parity corpus
- fix release ranking if the library owner still does not win

## Acceptance

- live warm-query timings materially improve against `g07.045`
- active corpus owner quality does not regress
- release orchestration result is either fixed or explicitly justified

## Evidence

- [`2026-05/18-182146-warm-query-latency-and-release-ranking.md`](../../../logs/archive/2026-05/18-182146-warm-query-latency-and-release-ranking.md)

## Next Task

Execute `998`.
