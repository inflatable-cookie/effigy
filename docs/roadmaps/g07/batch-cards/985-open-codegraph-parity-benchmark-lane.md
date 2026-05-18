# 985 - Open CodeGraph Parity Benchmark Lane

Roadmap: [`../036-parity-benchmark-harness-and-claim-discipline.md`](../036-parity-benchmark-harness-and-claim-discipline.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Open the parity lane with measurements before adding more graph features.

## Work

- create the gold query set for CodeGraph-parity assessment
- include architecture, ownership, route, call-flow, manifest, affected-test,
  cross-language, and exact-token cases
- record expected files and acceptable alternates for each query
- build or document a repeatable benchmark runner
- run the current `effigy graph explore` baseline
- write the baseline log under `docs/logs/2026-05/`
- update lane/front-door state if the benchmark changes the work order

## Acceptance

- `g07.036` has a baseline evidence log
- benchmark inputs are stable enough for later cards
- no percentage claim is made from a single hand-run comparison
- `986` is still valid or explicitly re-scoped

## Evidence

- [`codegraph-parity-gold-queries.toml`](../codegraph-parity-gold-queries.toml)
- [`2026-05/18-142849-codegraph-parity-benchmark-baseline.md`](../../../logs/2026-05/18-142849-codegraph-parity-benchmark-baseline.md)

## Next Task

Execute `986`.
