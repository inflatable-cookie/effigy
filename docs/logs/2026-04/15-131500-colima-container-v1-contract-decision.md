# Colima Container V1 Contract Decision

Date: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/006-colima-container-environment-strict-lane.md`
Batch Card: `docs/specs/batch-cards/106-decide-colima-container-v1-contract.md`

## Decision

Settle the v1 container contract now and move directly into implementation.

## Settled Contract

- `effigy container ...` is the primary product surface
- containers are declared in a named manifest registry under `[containers]`
- repos may declare a default container alias
- v1 stays Colima-first, with an optional named `profile` per environment
- host-facing access is explicit through manifest-declared `ports`
- file sharing is explicit through manifest-declared repo-relative `mounts`
- `primary_service` is the explicit shell/exec target
- no host DNS or service-discovery invention in v1
- attached sessions shut down on owner exit by default
- repo tasks may compose container control, but `effigy dev` stays repo-owned,
  not product magic

## Why

The machine-blocking problem is now specific enough that more planning would be
churn.

The critical execution assumptions are explicit:

- how a container is named and resolved
- how host access works
- what gets mounted
- where shell access goes
- who owns lifecycle and teardown

That is enough to implement a real first proof without letting code invent the
product.

## Remaining Deferred Limits

- no broad multi-driver abstraction yet
- no background daemon requirement
- no host DNS/service-registration layer
- no full per-service health DSL before the first proof
- no broad rollout before one real consumer implementation proof

## Outcome

`g02.006` now has a bounded, execution-ready contract and should proceed to the
first implementation batch.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Moved: the container lane shifted from high-level product intent into an
  execution-ready v1 contract with explicit host/service and lifecycle rules
- Remaining open: the implementation proof itself and any UX follow-up it
  exposes

## Next Task

Execute `docs/specs/batch-cards/107-implement-colima-container-foundation.md`
to build the first bounded Colima container foundation batch.
