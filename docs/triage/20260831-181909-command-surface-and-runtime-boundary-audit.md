# Command Surface And Runtime Boundary Audit

Status: open
Created: 2026-08-31
Owner: orchestrator conversation

## Observation

Effigy's public and internal feature surface has grown large. Much of the
breadth reinforces the single operator entry point, but the current shape may
also keep provider-specific or consumer-specific runtime weight inside the
binary without enough universality evidence.

The concrete trigger is `effigy-rhai`'s S3 storage host API. It pulls `s3`
directly, and the workspace currently carries a patched `vendor/s3` copy for
security fixes plus an upstream `quick-xml` constraint. That may be a legitimate
embedded runtime capability, or evidence that object-store execution belongs
behind a separate library, optional runtime, installed skill, or consumer app.

## Audit Question

Inventory current built-ins, manifest domains, embedded runtimes, shipped
catalogs, and provider dependencies. For each capability, distinguish:

- stable Effigy core or façade responsibility;
- reusable library/domain seam that should remain in the workspace;
- optional runtime/provider adapter that should not burden every installation;
- installed skill or extension candidate;
- consumer-owned workflow that Effigy should only invoke;
- deprecated or redundant surface that should be removed in a deliberate
  compatibility lane.

Judge boundaries by universality, routing value, safety/transaction ownership,
dependency and binary-weight cost, release coupling, provider specificity,
consumer evidence, and whether extraction preserves one obvious `effigy` entry
surface.

## Operator Direction

Confirmed 2026-08-31:

- cleaner ownership is the primary outcome;
- an unwieldy operator command surface is the second concern;
- dependency-tree growth and release coupling are material concerns;
- binary size is not important and should not drive extraction decisions;
- broad façade retention is preferable when it remains coherent, but the audit
  should challenge whether every current top-level family deserves that status.

## Known Context

- The CLI currently advertises more than thirty built-in command families.
- `docs/architecture/010-package-map.md` justifies current crate ownership, but
  it is not a feature-retention audit.
- `docs/architecture/022-runtime-architecture-sanity-audit.md` focused on
  runtime ownership migration, not product/core-versus-extension placement.
- `g04.039` rejustified crate boundaries and explicitly deferred object-store
  implementation.
- Research already sketches plugin/provider possibilities, but draft research
  is not current product authority.
- The active card `1089` touches docs/codegraph/CLI surfaces. This audit may run
  concurrently only as an isolated planning delegate with no canonical
  promotion or implementation until its packet is reviewed and merged.

## Open Decisions

- Must the `effigy` façade retain access to extracted capabilities, or may some
  commands disappear entirely?
- What compatibility appetite applies before `1.0`: immediate breaking
  cleanup, staged extraction, or evidence-only recommendations first?
- Should the first audit cover every feature or produce a ranked shortlist from
  representative pressure points such as S3/Rhai storage, deploy/state,
  containers/gateway, release/distribution, graph/scan/docs, and catalogs?

## Possible Home

A reviewed planning-delegate packet under `docs/triage/` or `docs/research/`,
then orchestrator-owned promotion into architecture/contracts and a future
roadmap only after operator decisions settle the target product boundary.
