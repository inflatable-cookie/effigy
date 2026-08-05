# g08.022 - Dependency Link Doctor And Hygiene

Status: Complete
Depends on: `g08.021`

## Goal

Make desired links, physical drift, and do-not-commit dependency state visible
through `effigy deps status` and `effigy doctor` without mutating either manager.

## Vision Alignment

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Target envelope: every unhealthy link state names the affected manager,
  library, packages, evidence, and remediation.
- Vision target delta: local dependency overlays become observable repo health
  state instead of invisible developer-machine behavior.

## Scope

- surface healthy desired links as doctor information
- report missing library paths and partial closures as errors
- report tracked/conflicting Cargo config as errors
- report Cargo path-linked lock state as do-not-commit errors
- report complete Bun symlink-closure loss as a repairable warning
- report mixed local/registry closure and registration-index conflicts as errors
- report Bun manifest/lock mutation and incompatible duplicate peers as errors
- keep doctor read-only and reuse the dependency-domain inspector
- align text and JSON fields between deps status and doctor evidence
- add remediation-first output for link, unlink, re-link, and manual cleanup

## Non-Goals

- no automatic doctor fix mode
- no package-manager install/update execution from doctor
- no weakening errors merely to make linked development report green

## Execution Plan

- [x] [`1060`](./batch-cards/1060-observe-dependency-hygiene-and-status-parity.md)
      — extend the shared read-only inspector and status surface with Cargo
      hygiene, Bun drift, registration, immutable-file, and peer evidence
- [x] [`1061`](./batch-cards/1061-integrate-dependency-health-with-doctor-and-closeout.md)
      — adapt the shared observations into doctor findings, prove parity, and
      close this milestone

## Acceptance Criteria

- [x] doctor distinguishes healthy, warning, and error states per contract 034
- [x] findings carry manager, mechanism, library path, packages, and evidence
- [x] Cargo do-not-commit state is explicit and names unlink/restore workflow
- [x] Bun drift names the idempotent re-link command
- [x] Bun full-loss drift and partial-closure failure have distinct severity
- [x] doctor performs no writes or manager mutations
- [x] standard doctor JSON remains envelope/schema compatible
- [x] status and doctor cannot disagree on observed link state for one fixture

## Validation

- focused doctor finding/severity/render tests
- Cargo lock/config hygiene fixtures
- Bun drift/peer fixtures
- doctor text/JSON integration tests
- `effigy qa:ci:fast`

## Next Task

Execute ready portfolio Cargo proof card `1062` under `g08.023`.
