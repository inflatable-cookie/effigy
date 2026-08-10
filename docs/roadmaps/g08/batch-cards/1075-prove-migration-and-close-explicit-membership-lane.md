# 1075 - Prove Migration And Close Explicit Membership Lane

Roadmap: [`../028-explicit-catalog-membership.md`](../028-explicit-catalog-membership.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/037-explicit-catalog-membership-contract.md`](../../../contracts/037-explicit-catalog-membership-contract.md)
Spec: [`../../../specs/archive/101-explicit-catalog-membership-strict-lane.md`](../../../specs/archive/101-explicit-catalog-membership-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-08-10
Ready after: card 1074

## Purpose

Prove the breaking migration across representative project shapes, publish the
new operator contract, and close every strict-lane surface with full evidence.

## Owner And Seam

The consumer-adoption and closeout seam owns this card. Runtime behavior stays
under contract `037`; this card proves it, documents it, and closes planning
state without adding new membership semantics.

## Work

- update README, routing/test/system guides, cookbook, troubleshooting, command
  matrix, agent skill references, init starters, and examples
- add an `[Unreleased] > Breaking` changelog entry with direct migration steps
- prove current Effigy self-host behavior without discovery ignore config
- prove root-only, nested descendant, symlinked member, sibling member, named
  mount reference, inline catalog mount, and ordinary mount fixtures
- capture text/JSON tasks and test-plan evidence where contracts require it
- run full QA after focused proof
- close roadmap `g08.028`, cards, spec `101`, active front doors, and one dated
  evidence log; archive the spec when no longer needed as active context

## Acceptance

- [x] public docs consistently teach explicit membership
- [x] migration guidance covers nested and mounted catalogs without a scanner
- [x] changelog declares config, behavior, and command removals
- [x] self-host and every contract-required consumer shape pass
- [x] full QA and JSON contracts pass
- [x] no discovery-era live guidance remains outside historical records
- [x] roadmap/spec/front doors show complete with no stale ready card
- [x] no release or workflow mutation occurs

## Validation

- focused self-host and consumer-shape fixtures
- `effigy doctor`
- `effigy tasks`
- `effigy test --plan`
- `effigy qa:ci:fast`
- `effigy qa:docs`
- `effigy qa:json`
- full `effigy qa`
- `git diff --check`

## Evidence

Recorded in
[`2026-08/10-105636-explicit-catalog-membership-closeout.md`](../../../logs/2026-08/10-105636-explicit-catalog-membership-closeout.md).

## Stop Conditions

Stop before inventing new membership grammar, adding a migration scanner,
editing workflows, running release mutations, or changing another repo outside
disposable proof fixtures.

## Next Task

Lane complete. Await the next operator-approved g08 scope. Do not infer a
release or generation rollover.
