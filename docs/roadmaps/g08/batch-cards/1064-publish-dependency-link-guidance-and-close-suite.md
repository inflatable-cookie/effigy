# 1064 - Publish Dependency Link Guidance And Close Suite

Roadmap: [`../023-dependency-link-portfolio-proof-and-closeout.md`](../023-dependency-link-portfolio-proof-and-closeout.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1063`

## Purpose

Turn the proven Cargo and Bun behavior into concise operator and agent guidance,
then close `g08.023`, the `g08.018` suite, and strict lane `099`.

## Owner And Seam

Durable behavior remains in architecture `023` and contract `034`. This card
owns guide, help/reference, JSON-example, agent-routing, evidence, and planning
closeout surfaces only.

## Work

- publish one local dependency-linking operator guide covering committed truth,
  desired state, dry-run, link, status, doctor, unlink, and recovery
- document Cargo closure/config/lock hygiene and nested-workspace behavior
- document Bun save-less registration, install drift, re-link, ownership, and
  peer dedupe
- update command/help matrices, JSON examples/schema index, README, and the
  bundled Effigy agent skill where needed
- consolidate Cargo and Bun proof evidence and known limits
- close `g08.023`, `g08.018`, strict lane `099`, and all roadmap front doors

## Guardrails

- no release execution or workflow edits
- no new package manager or speculative dependency subcommand
- no duplicate normative behavior outside architecture/contract authority
- no claim of real published TS portfolio proof before publication exists

## Acceptance

- [x] operator path is complete from dry-run through recovery
- [x] Cargo and Bun hazards/remediation are explicit
- [x] text and JSON examples match the shipped command contract
- [x] agent guidance routes dependency-link health through status and doctor
- [x] full QA passes or an independent upstream blocker is recorded precisely
- [x] suite, strict lane, and front doors close with one explicit next task

## Evidence

- [`Local dependency linking guide`](../../../guides/077-local-dependency-linking.md)
- [`Dependency linking suite closeout`](../../../logs/2026-08/05-231121-dependency-linking-suite-closeout.md)
- Cargo proof: [`1062`](./1062-prove-signal-links-across-flat-and-nested-consumers.md)
- Bun proof: [`1063`](./1063-prove-bun-closure-drift-and-repair.md)

## Validation

- command/help/JSON example contract checks
- `effigy qa:docs`
- `effigy qa:ci:json`
- `effigy qa`
- `git diff --check`

## Stop Conditions

Stop and replan if guide work exposes an unresolved product gap, proof evidence
is incomplete, or closing the strict lane would leave any ready dependency-link
batch unexecuted.

## Next Task

Select the next substantial g08 scope separately. Do not infer a release or
generation rollover.
