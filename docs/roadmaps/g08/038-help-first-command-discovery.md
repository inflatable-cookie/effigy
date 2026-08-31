# g08.038 Help-First Command Discovery

Status: Complete
Created: 2026-08-31
Completed: 2026-08-31
Evidence: [`2026-08-31 closeout`](../../logs/2026-08/31-233000-help-first-command-discovery-1093.md)
Spec: [`111`](../../specs/archive/111-help-first-command-discovery-strict-lane.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Make Effigy's broad command surface easier to discover without making its
execution grammar larger or disturbing manifest-selector routing.

## Decisions

- Group general help under exact topics `work`, `local`, `repo`, `deliver`,
  `extend`, and `admin`.
- Add `effigy help <group>` and conventional `effigy help <command>` detail.
- Keep all executable commands and selector precedence unchanged.
- Add no `effigy <group> <command>` aliases and deprecate nothing.
- Give every general-help entry one primary group.

## Scope

- typed primary-group ownership in the CLI command/help inventory
- grouped `effigy --help` and `effigy help`
- exact group-topic inventories
- direct command help through `effigy help <command>`
- deterministic unknown-topic diagnostics
- deferral and selector-collision proof
- public, generated, and agent-facing documentation parity
- changelog, validation, evidence, and closeout

## Boundary

- no executable grouped aliases or new top-level built-in names
- no direct-command grammar, behavior, JSON, or exit changes
- no alias hiding, warning, deprecation, or removal
- no release, catalog-pack, S3, or provider extraction work
- no taxonomy expansion without a new operator decision

## Cards

- [x] [`1093`](./batch-cards/1093-add-help-first-command-discovery.md) — complete

## Acceptance

- general help is grouped by the six approved operator jobs
- each group topic exposes only its primary inventory
- command detail agrees with the existing `<command> --help` owner
- deferred built-ins remain absent from applicable help surfaces
- group words remain available to manifest tasks under current routing
- focused and full validation pass
- one evidence log closes the lane and returns the queue to planning

## Next Task

This milestone is complete. Return to planning for the catalog-pack acquisition
prototype under contract
[`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md);
do not open that implementation lane from here.
