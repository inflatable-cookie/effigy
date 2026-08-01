# g08.018 - Apple Containers Native Backend Prototype

Status: Paused — watch-only decision recorded
Depends on: `g08.017`
Created: 2026-08-01

## Goal

Prove whether Apple Containers 1.2 can satisfy Effigy's generated local-stack
contract on Apple silicon without pretending to implement Compose. Keep Docker
Compose and Colima behavior stable while introducing the backend-neutral plan
and live evidence needed for a support decision.

## Generation Runway

This lane advances g08's open platform-hardening runway: remove an accidental
Compose boundary from the container manager, prove one native runtime adapter,
then decide support from live compatibility and resource evidence.

The planning checkpoint is after Batch C. At that point Apple Containers is
either promoted to an explicit experimental backend, held as watch-only with
named upstream gaps, or rejected with reproducible evidence.

## Scope

- Effigy-generated catalog stacks only
- Apple Containers 1.2 on Apple silicon and macOS 26
- explicit prototype selection only; no automatic detection
- app/web/database/cache proof stack
- lifecycle, exec, logs, mounts, ports, networking, readiness, gateway, and
  volume evidence
- Docker/Colima regression protection

## Non-Goals

- arbitrary `compose_file` translation
- general Compose compatibility
- Docker socket/API emulation
- replacing Docker or Colima defaults during the prototype
- global Apple system DNS as a required service-discovery dependency
- release or CI workflow mutation

## Execution Plan

- [x] **Batch A — Effective stack plan and live runtime baseline.** Installed the
  signed Apple Containers 1.2 package, recorded the CLI/runtime baseline, and added
  a typed stack-plan seam for generated catalog services without changing the
  public backend set or existing Compose output.
- [x] **Batch B — Native operation adapter and bounded stack lifecycle.** Added an
  explicitly selected `apple-container` prototype adapter that consumes the
  stack plan and proves build/pull, network, create/start/readiness, exec, logs,
  inspect/ports, stop, restart, and remove for one representative stack.
- [x] **Batch C — Compatibility, resource, and promotion decision.** Proved or
  explicitly classified
  service discovery, interrupted recovery, gateway routing, volume lifecycle,
  SSH/Rosetta behavior, and four-to-six-service resource cost against Docker
  and Colima; the support decision is watch-only.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)
- [`006-compose-backend-compatibility.md`](../../contracts/006-compose-backend-compatibility.md)
- [`012-container-manager-contract.md`](../../contracts/012-container-manager-contract.md)
- [`015-runtime-operation-pipeline-contract.md`](../../contracts/015-runtime-operation-pipeline-contract.md)
- [`Translation Memo 017`](../../research/translation-memos/017-apple-containers-runtime-backend.md)
- [`Strict lane 099`](../../specs/099-apple-containers-native-backend-prototype.md)

## Acceptance Criteria

- [x] generated catalog stacks have one typed semantic plan used by both
  Compose rendering and Apple operation planning
- [x] direct Compose input fails before Apple runtime mutation with a clear
  Docker/Colima remedy
- [ ] the representative stack supports lifecycle, routed exec, readiness,
  service discovery, gateway registration, and preserved data
- [x] interrupted and stale-state recovery is deterministic
- [x] Docker and Colima behavior and tests remain green
- [x] comparative startup, memory, disk, and I/O evidence is recorded
- [x] the final support decision and remaining limitations are explicit

## Next Task

No execution card is ready. Keep the lane paused and watch-only until guide
`077`'s boot-time discovery reassessment gate is met.
