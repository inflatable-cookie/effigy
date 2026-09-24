# Roadmap Generation Index

Current generation: g10
Approved frontier: g10.014 ready; g10.015 approved after g10.014
Updated: 2026-09-24

## Lifecycle state

Generations `g01` through `g09` are safely closed and compacted. The operator
selected an agent-native skill-execution runway on 2026-09-13. `g10` remains
open; `g10.001` through `g10.013` are complete. The 0.13.0 release is published
and install-verified. The operator then selected consolidated resolution of
ten open Dependabot PRs. `g10.014` owns six compatible lockfile updates;
`g10.015` owns the three direct upgrades and overlapping language update.
The shared lockfile requires serial execution.

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
- Future Effigy release mutation remains explicitly operator-gated.

## Next Task

Dispatch `g10.014` through Northstar Queue, then `g10.015` after terminal
closeout. Choose the next strategic runway with the operator afterward.
