# Roadmap Generation Index

Current generation: g10
Approved frontier: g10.007
Updated: 2026-09-15

## Lifecycle state

Generations `g01` through `g09` are safely closed and compacted. The operator
selected an agent-native skill-execution runway on 2026-09-13. `g10` remains
open; `g10.001` through `g10.005` are complete. The operator promoted catalog-
scoped monorepo graph indexing as `g10.006` on 2026-09-15, then promoted
lockfile-bounded `g10.007` as its base-maintenance prerequisite after PR #109
was blocked by pre-existing cargo-deny findings.

## Generation history

| Generation | State | Durable scope |
| --- | --- | --- |
| [`g01`](./archive/g01.md) | archived | original runner implementation, consolidation, scans, and Northstar adoption |
| [`g02`](./archive/g02.md) | archived | release, local runtime, containers, gateway, data, and multi-project operation |
| [`g03`](./archive/g03.md) | archived | production export, runtime convergence, distribution, and artifacts |
| [`g04`](./archive/g04.md) | archived | typed runtime, state, deployment, provider, and ownership simplification |
| [`g05`](./archive/g05.md) | archived | secrets, local configuration, gateway trust, and reusable-core hardening |
| [`g06`](./archive/g06.md) | archived | bounded codebase lean-down and ownership simplification |
| [`g07`](./archive/g07.md) | archived | native code graph, query quality, setup, and agent adoption |
| [`g08`](./archive/g08.md) | archived | graph-aware scans, dependencies, release integrity, docs, and catalog packs |
| [`g09`](./archive/g09.md) | archived | operator/consumer clarity, docs-context proof, and local-link recovery |
| [`g10`](./g10/README.md) | active | agent-native execution, bounded operability, and catalog-scoped repository intelligence |

## Rollover history

`g01` → `g02` → `g03` → `g04` → `g05` → `g06` → `g07` → `g08` → `g09`
was the completed sequential history. Each rollover was operator-owned. The
2026-09-09 flattened-task switchover compacted all nine already-closed expanded
trees without opening another generation.

## Retained commitments

- Consumer cohort expansion remains unscheduled under vision artifacts `007`
  and `020`.
- Optional-provider/S3 retirement remains in triage `20260901-092640`.
- Release keep-on-failure and hosted-evidence delegation remain in triage
  `20260905-092527` and `20260906-224721`.
- Effigy release mutation remains explicitly operator-gated.

## Next Task

Dispatch [`g10.007`](./g10/007-refresh-vulnerable-and-yanked-cargo-lock.md)
through Northstar Queue. After it closes, resume the retained `g10.006` Queue
task to rebase and revalidate PR #109.
