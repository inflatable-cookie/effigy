# 046 - Container Operation Pipeline Strict Lane

Roadmap: [`g04.004`](../roadmaps/g04/004-container-operation-pipeline.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Make container command surfaces thin wrappers over typed operation requests and
plans.

`g04.003` moved runtime activation into a shared request/plan/stage path. The
next pressure point is direct container operation orchestration: lifecycle,
exec, shell, logs, status, stats, data, and cache flows still build too much
backend and safety behavior in runner-local code.

## Hard Boundaries

- no public CLI behavior changes unless a card explicitly selects a cleanup
  break
- no release work
- no `.github/workflows/` edits
- do not move Docker, Colima, or nerdctl branching into runner code
- keep rendering in runner unless a dependency-light renderer boundary becomes
  obviously useful

## Current Ready Card

None. This lane is complete.

## Execution Chain

- `469` complete: close runtime activation pipeline and hand off container
  operations
- `470` complete: scaffold container operation pipeline lane
- `471` complete: add container ops lifecycle plan foundation
- `472` complete: wire lifecycle operation plans into runner glue
- `473` complete: select next container operation family
- `474` complete: add container read operation plans
- `475` complete: wire read operation plans into runtime glue
- `476` complete: select next container operation slice
- `477` complete: add container exec shell operation plans
- `478` complete: wire exec shell operation plans into runner glue
- `479` complete: select data/cache or manager migration
- `480` complete: add container data cache operation plans
- `481` complete: wire data cache operation plans into runtime glue
- `482` complete: select container manager migration or closeout
- `483` complete: add manager compose invocation plan foundation
- `484` complete: wire manager compose plan into runtime read callers
- `485` complete: wire manager compose plan into lifecycle down reset
- `486` complete: select exec shell or data cache manager migration
- `487` complete: wire manager compose plan into captured exec
- `488` complete: wire manager compose plan into interactive shell
- `489` complete: select attached session or data cache manager migration
- `490` complete: wire manager compose plan into attached session
- `491` complete: select data cache or gateway support manager migration
- `492` complete: wire manager compose plan into data pull production
- `493` complete: select gateway support image cleanup or up migration
- `494` complete: wire manager compose plan into container up
- `495` complete: select gateway support image cleanup or shared service migration
- `496` complete: wire manager compose plan into gateway tcp alias hosts
- `497` complete: select shared service or generated image cleanup migration
- `498` complete: wire manager compose plan into shared service bring up
- `499` complete: wire manager runtime plan into generated image cleanup
- `500` complete: review container operation drift and closeout
- `501` complete: remove final runner compose runtime helper drift

## Exit Condition

This lane closes when container lifecycle, exec, shell, logs, status, stats,
data, and cache commands are represented as typed operation requests/plans, and
runner command modules no longer own backend command construction for those
surfaces.

## Next Task

Continue with
[`047-data-seed-dump-pipeline-strict-lane.md`](./047-data-seed-dump-pipeline-strict-lane.md).
