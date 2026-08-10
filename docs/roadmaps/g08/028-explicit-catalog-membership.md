# g08.028 - Explicit Catalog Membership

Status: Complete
Depends on: `g08.027`
Contracts: [`001`](../../contracts/001-working-rules.md),
[`037`](../../contracts/037-explicit-catalog-membership-contract.md)
Spec: [`101`](../../specs/archive/101-explicit-catalog-membership-strict-lane.md)

## Goal

Replace recursive ambient catalog discovery with explicit root-owned
membership while preserving cwd-aware invocation, selector precedence, system
mount behavior, and test orchestration across the declared catalog set.

## Vision Alignment

- Primary tags: `ROUTE`, `CONTRACT`, `MAINT`, `OPERATE`
- Target envelope: catalog membership is deterministic configuration rather
  than filesystem side effect.
- Vision target delta: every routed catalog has declaration evidence; nested
  manifests and ordinary mounts no longer alter the task surface implicitly.

## Goals

- [x] add named catalog members and typed system/workspace mounts
- [x] normalize named and inline members through one routing-owned model
- [x] cut every catalog consumer over without changing selector precedence
- [x] delete descendant walking, discovery config, caches, and catalog cache CLI
- [x] migrate self-host and public guidance to the breaking explicit grammar
- [x] prove nested, symlink, sibling, mounted, and ordinary-mount boundaries

## Execution Plan

- [x] card 1072: add explicit member and typed mount schema with string parity
- [x] card 1073: cut routing and all catalog consumers over to explicit
      membership
- [x] card 1074: delete discovery/cache surfaces and align doctor, CLI, and JSON
      diagnostics
- [x] card 1075: prove migration shapes, publish the break, run full QA, and
      close the lane

## Owner And Seam

- `effigy-manifest` owns member and mount grammar
- `effigy-routing` owns effective membership, canonical identity, loading,
  ordering, aliases, and evidence
- `effigy-containers` renders typed mounts without owning membership
- doctor, CLI, test, demo, status, and runner surfaces consume shared models

Contract `037` is authoritative when roadmap prose is less specific.

## Non-Goals

- no descendant glob or recursive migration scanner
- no ambient fallback or compatibility resolver
- no recursive child-member expansion
- no selector precedence or unique-task ownership rewrite
- no system-selected task surface
- no workflow edit or release mutation

## Acceptance Criteria

- [x] `[catalog.members]` and both structured mount forms match contract `037`
- [x] ordinary and legacy mounts never imply membership
- [x] task availability is stable across selected systems/workspaces
- [x] canonical path deduplication and alias conflicts retain exact evidence
- [x] all catalog consumers share the routing-owned set
- [x] descendant walks, discovery configuration, caches, and cache CLI are gone
- [x] current repo and representative consumer shapes pass focused and full QA
- [x] changelog and public docs provide a direct migration path
- [x] spec `101` and the active front doors close without a stale ready card

## Runway

- completed explicit-membership lane: `1072` through `1075`
- closeout checkpoint: focused consumer proof, fast CI, full QA, docs, and JSON
  contracts pass
- no ready card remains in this lane

## Stop Conditions

Stop and replan if:

- runtime correctness needs descendant walking or glob expansion
- structured mounts cannot preserve existing string-mount rendering
- membership depends on the selected runtime system
- a second resolver is needed outside `effigy-routing`
- consumer migration requires an ambient compatibility mode
- work reaches workflow or release mutation

## Next Task

Lane complete. Await the next operator-approved g08 scope; do not infer a
release or generation rollover.
