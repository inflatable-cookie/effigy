# 015 Persistent Data And Volume Lifecycle Strict Lane

Status: active
Updated: 2026-04-18
Roadmap: `g02.015`

## Context

`g02.016` is now closed. The next open integration lane is persistent data.
The catalog layer already knows about named volumes and reset retention, but
the product surface still behaves like data is disposable.

This lane owns the integration path that turns that substrate into real
operator behavior.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/015-persistent-data-and-volume-lifecycle.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

This lane owns:

- product-owned persistent data lifecycle on the container path
- bounded data-management surfaces where Effigy can act honestly
- follow-through on the shipped catalog volume substrate
- task-owned seeding and production-pull hooks only after the lifecycle
  foundation is real

## Current Posture

`active`

Shipped substrate that this lane builds on:

- catalog fragments already declare named volumes plus `persist` retention
  metadata
- generated compose assembly already includes those named volumes on the
  product path
- `effigy-catalog::volumes` already has reset classification plus Docker
  command specs for list/export/import
- the container runner already owns `down` and `reset`, but still only exposes
  the all-or-nothing reset path

## Integration Constraint

This lane should start with the narrowest trustworthy lifecycle slice before
widening into transfer or hook orchestration:

- make `container reset --keep-data` real before adding more data commands
- keep the first batch on generated-compose ownership, where volume retention
  metadata already exists
- treat direct `compose_file` data lifecycle, import/export, and pull hooks as
  later widening decisions rather than assumed first-batch scope

## Remaining Integration Work

The bounded continuation chain now starts with:

1. `279` — plan the first `g02.015` execution batch on the actual shipped
   volume substrate
2. `280` — first execution batch: generated-compose persistent reset
   foundation through `effigy container reset --keep-data`
3. `281` — decide the next widening step now that the first lifecycle surface
   is real
4. `282` — bounded volume inventory through `effigy container data list`

What is now real in the product path:

- `effigy container reset --keep-data` on the generated-compose path
- persistent-vs-ephemeral volume classification from shipped catalog metadata
- honest text/JSON reporting of which volumes were kept vs removed
- explicit rejection or bounded fallback for direct `compose_file` ownership
  where Effigy does not have trustworthy retention metadata yet

The next bounded widening is now explicit too:

- `effigy container data list` for one environment
- read-only inventory before transfer or hook orchestration
- export/import, media lifecycle, and `pull_production` still left for later
  planning after inventory is real

## Exit Condition

This strict lane is complete when:

- the product has a real persistent-data lifecycle surface instead of only
  hidden substrate
- any wider data-management or hook behavior is either shipped or explicitly
  bounded/deferred on a trustworthy product boundary

## Next Task

Execute `282` to land bounded `effigy container data list` inventory.
