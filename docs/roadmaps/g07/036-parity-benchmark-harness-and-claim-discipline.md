# g07.036 - Parity Benchmark Harness And Claim Discipline

Status: Complete
Depends on: `g07.035`

## Goal

Create the repeatable benchmark harness that governs every CodeGraph-parity
claim.

## Scope

- define 8 to 12 gold navigation tasks across:
  - ownership lookup
  - architecture explanation
  - call-flow tracing
  - route/entrypoint tracing
  - manifest/task routing
  - changed-file impact
  - cross-language flow
  - exact-token lookup
- record expected owner files, acceptable alternates, and known false positives
- measure:
  - graph calls
  - `rg` calls
  - file reads
  - elapsed time
  - output bytes
  - stale-index posture
- create a local harness command or script under repo-owned tooling; prefer
  Rust or existing Effigy tasks over shell glue if the logic becomes structured
- store benchmark logs under `docs/logs/YYYY-MM/`
- define which comparisons are fair:
  - warm index vs warm index
  - cold index vs cold index
  - exact-match tasks separated from navigation tasks

## Guardrails

- no percentage claim unless the harness produces it
- no cherry-picking only passing queries
- no comparing `graph explore` against deliberately inefficient manual scans
- no marking a result "zero reread" unless returned excerpts were enough for a
  plausible implementation or explanation
- no release-facing claim before at least one full closeout run

## Acceptance Criteria

- benchmark task file exists and is human-readable
- harness output is machine-readable enough for trend comparison
- first baseline log records current Effigy graph behavior
- every later roadmap has a stable measurement target

## Evidence

- [`codegraph-parity-gold-queries.toml`](./codegraph-parity-gold-queries.toml)
- [`2026-05/18-142849-codegraph-parity-benchmark-baseline.md`](../logs/2026-05/18-142849-codegraph-parity-benchmark-baseline.md)

## Next Task

Execute `986` after the harness pins the baseline and gold query set.
