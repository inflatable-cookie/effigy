# 1061 - Integrate Dependency Health With Doctor And Closeout

Roadmap: [`../022-dependency-link-doctor-and-hygiene.md`](../022-dependency-link-doctor-and-hygiene.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1060`

## Purpose

Adapt the shared dependency-health report into doctor findings, prove status
and doctor parity, and close `g08.022` without adding a second inspector.

## Owner And Seam

`effigy-deps` remains the observation authority. `effigy-doctor` maps shared
findings into doctor severity and remediation. The runner only dispatches and
renders established contracts.

## Work

- add a read-only doctor adapter over the shared observed-health report
- map healthy links to information, repairable Bun full loss to warning, and
  partial closure, conflicts, do-not-commit state, and duplicate peers to errors
- preserve manager, mechanism, library, package, evidence, and remediation in
  doctor text and JSON
- prove status and doctor classify identical fixtures identically
- prove doctor performs no writes or mutating package-manager processes; Cargo
  resolution remains a read-only `cargo metadata` observation owned by
  `effigy-deps`
- close `g08.022` and ready the first bounded `g08.023` proof card

## Guardrails

- no duplicate Cargo/Bun inspection in doctor
- no doctor fix mode or manager mutation
- no portfolio consumer mutation in this card
- no weakening unhealthy linked-development states to pass health checks

## Acceptance

- [x] doctor severity follows contract 034 for every shared finding kind
- [x] doctor text and JSON retain exact dependency-health evidence
- [x] status and doctor agree across healthy and unhealthy fixtures
- [x] doctor performs no writes or mutating manager processes
- [x] `g08.022` closes with a ready `g08.023` batch card

## Validation

- focused doctor adapter/severity/render fixtures
- status/doctor parity integration tests
- `effigy qa:ci:fast`
- `effigy qa:ci:json`
- `effigy qa:docs`
- `git diff --check`

## Stop Conditions

Stop and replan if doctor needs a second manager inspector, standard doctor JSON
cannot carry the evidence additively, or health checks would mutate local link
state.

## Next Task

Execute ready portfolio Cargo proof card `1062`.
