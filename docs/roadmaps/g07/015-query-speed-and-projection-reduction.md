# g07.015 - Query Speed And Projection Reduction

Status: Complete
Depends on: `g07.013`

## Goal

Reduce graph query latency enough that the graph stays competitive for normal
agent lookup, especially for `status`, `search`, and `context`.

## Scope

- reduce obvious whole-table reloads in the query path
- avoid repeated projection work when the command only needs a thin result view
- tighten `graph context` assembly so ranking and snippet selection do not do
  unnecessary broad work
- measure the result against the `g07.012` baseline

## Guardrails

- no schema break just to chase speed
- no stale implicit caches
- no ranking simplification that makes `graph context` less predictable
- no removal of reasons, provenance, or overflow accounting

## Acceptance

- query latency improves measurably from the `g07.012` baseline
- command behavior and JSON contracts stay stable
- `graph context` remains bounded and deterministic

## Next Task

Execute `935`.
