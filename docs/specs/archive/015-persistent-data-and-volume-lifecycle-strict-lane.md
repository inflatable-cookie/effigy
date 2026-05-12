# 015 Persistent Data And Volume Lifecycle Strict Lane

Status: complete
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

`complete`

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
5. `283` — choose the next bounded widening step after landed inventory
6. `284` — bounded generated-compose transfer through `data export/import`
7. `285` — choose the next bounded widening step after landed transfer
8. `286` — bounded media bind-mount lifecycle on the generated-compose path
9. `287` — choose the next bounded widening step after landed media lifecycle
10. `288` — bounded generated-compose `data.pull_production` hook ownership
11. `289` — decide whether the lane closes now or needs one real-project proof
12. `290` — prove the generated-compose persistent-data loop in one real project

What is now real in the product path:

- `effigy container reset --keep-data` on the generated-compose path
- persistent-vs-ephemeral volume classification from shipped catalog metadata
- honest text/JSON reporting of which volumes were kept vs removed
- `effigy container data list` for one generated-compose environment with
  bounded runtime size and mount metadata when the runtime can provide it
- bounded generated-compose `data export` and `data import` for explicit
  managed volume names
- bounded generated-compose `[containers.<name>.data].media` declarations with
  repo-owned directory preparation and compose mounts on repo-bound services
- bounded generated-compose `[containers.<name>.data].pull_production`
  ownership through one product entrypoint and repo-relative shell/Rhai hooks
- explicit rejection or bounded fallback for direct `compose_file` ownership
  where Effigy does not have trustworthy retention metadata yet

The final proof is now real:

- task-owned seeding stayed on the shipped task, workspace binding, exec, and
  Rhai surfaces rather than widening into a new product abstraction batch
- one bounded real-project proof landed through `farmyard`
- the proof exposed and closed two real product gaps: runtime volume-name
  drift on generated compose, and stale generated compose reuse when only
  assembly logic changed

## Exit Condition

This strict lane is complete when:

- the product has a real persistent-data lifecycle surface instead of only
  hidden substrate
- any wider data-management or hook behavior is either shipped or explicitly
  bounded/deferred on a trustworthy product boundary

## Next Task

No further execution lives on `g02.015`. Stop in planning and choose the next
remaining `g02` lane deliberately.
