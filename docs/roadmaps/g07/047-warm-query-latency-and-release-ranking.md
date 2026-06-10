# g07.047 - Warm Query Latency And Release Ranking

Status: Complete
Depends on: `g07.046`

## Goal

Recover warm `graph explore` latency on the live Effigy repo without regressing
the ranking gains from the parity suite.

## Scope

- profile current warm-query cost on the active parity corpus
- identify the dominant hot path in query assembly and traversal
- reduce repeated storage scans, broad relation expansion, or packet assembly
  work where the evidence shows it matters
- preserve deterministic output and current JSON contracts
- rerun the release-architecture query and fix ranking if the release library
  owner still loses to CLI parsing

## Guardrails

- no feature detour unrelated to the measured latency regression
- no deleting reasons, provenance, freshness, or overflow evidence for speed
- no hidden stale-index shortcut
- no claim that a speedup matters unless it shows up on the live parity corpus

## Acceptance Criteria

- warm-index parity queries are materially faster than the `g07.045` closeout
- current owner quality does not regress on the active corpus
- `understand release orchestration` either promotes the release library owner
  or records an explicit measured reason not to

## Evidence

- [`2026-05/18-182146-warm-query-latency-and-release-ranking.md`](../../logs/archive/2026-05/18-182146-warm-query-latency-and-release-ranking.md)

## Next Task

Execute `998`.
