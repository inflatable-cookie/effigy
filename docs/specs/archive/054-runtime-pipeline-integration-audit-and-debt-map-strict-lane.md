# 054 - Runtime Pipeline Integration Audit And Debt Map Strict Lane

Roadmap: [`g04.012`](../roadmaps/g04/012-runtime-pipeline-integration-audit-and-debt-map.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Map the integration debt left after the first `g04` pipeline sweep so the next
implementation cards remove real bypasses instead of creating more planning
surface.

## Hard Boundaries

- no release work
- no `.github/workflows/` edits
- no broad implementation refactor in this audit lane
- keep public CLI behavior unchanged
- classify drift allowances honestly as adapter boundary or migration debt

## Current Ready Card

[`579-open-runtime-activation-route-authority-lane.md`](./batch-cards/579-open-runtime-activation-route-authority-lane.md)

## Execution Chain

- `578` complete: map runtime pipeline integration debt
- `579` ready: open runtime activation route authority lane

## Audit Areas

- drift-guard allowances
- activation-plan construction and `RuntimeActivationRoute` use
- `DataSeedPlan` / `DataDumpPlan` use versus low-level helper calls
- `ContainerOperationPlan` values that do not govern execution
- named-volume and orphan filtering integration with container operations
- architecture guard QA coverage
- large new planning crate files

## Debt Map

| Area | Current state | Classification | Next owner |
| --- | --- | --- | --- |
| activation route identity | `RuntimeActivationRoute` exists, but `RuntimeActivationPlan::from_request()` defaults every plan to `Task`; runner callers duplicate builder code | migration debt | `g04.013` |
| activation builder duplication | `db_seed`, `deferral`, `exec`, `standard`, `managed`, and `container_runtime_prep` each build nearly identical activation requests | migration debt | `g04.013` |
| backend branching drift | `doctor_ports`, `exec_command/transport`, `container_runtime_prep/prep`, and `container_command/lifecycle` remain allowlisted | mixed: doctor is adapter/diagnostic boundary; others are migration debt | `g04.013`/later manager cleanup |
| raw container CLI drift | `doctor_ports` and `bootstrap_command/mod.rs` remain allowlisted | mixed: doctor diagnostic boundary; bootstrap detection is migration debt | later manager cleanup |
| `compose_args` runner callers | multiple runner surfaces still render compose args directly for lifecycle cleanup, inline shells, deferral, exec, workspace provisioning, demo selection | migration debt except explicit render-only helpers | later manager cleanup after route/data work |
| legacy container exec capture | `db_seed`, `container_command/data`, `container_command/lifecycle`, and `container_command/mod` remain allowlisted | migration debt | `g04.014` plus later manager cleanup |
| data seed/dump plans | `DataSeedPlan` and `DataDumpPlan` exist, but runner uses lower-level helpers directly | migration debt | `g04.014` |
| operation plans discarded | `_operation_plan` appears across read/data/cache/lifecycle/exec paths; plans prove intent but often do not govern execution/reporting | migration debt | `g04.015` and later manager cleanup |
| volume operations | recent `container volume list`, orphan filtering, and volume inventory are runtime/data-adjacent, not first-class container ops | migration debt | `g04.015` |
| architecture guard coverage | `qa:architecture` exists but is not wired into `qa:gates`, `qa:ci`, `qa`, or `prepush:ci` | migration debt | `g04.016` |
| new planning crate size | `effigy-data`, `effigy-containers`, `effigy-execution`, and `effigy-artifacts` are large single-file crates | structural debt; split after integration is real | `g04.017` |

## Selected Implementation Order

1. `g04.013` - fix activation route identity and shared activation builder.
2. `g04.014` - make data seed/dump flows consume full plans.
3. `g04.015` - add volume operations to the container operation pipeline.
4. `g04.016` - wire architecture guards into normal validation.
5. `g04.017` - split large planning crates once integrations settle.

## Exit Condition

This lane closes when the debt map is concrete, each major debt class has a
selected roadmap/card, and the first implementation lane is ready.

## Next Task

Card
[`579-open-runtime-activation-route-authority-lane.md`](./batch-cards/579-open-runtime-activation-route-authority-lane.md).
