# 290 Prove Generated Compose Persistent Data Loop In One Real Project

Status: archived
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Prove the shipped generated-compose persistent-data contract in one real
project before closing `g02.015`.

## Context

`280`, `282`, `284`, `286`, and `288` landed the bounded product surface:

- generated-compose `container reset --keep-data`
- `container data list`
- `container data export/import`
- manifest-owned `data.media`
- manifest-owned `data.pull_production`

`289` decided that task-owned seeding does not need another product abstraction
batch. The remaining confidence gap is consumer proof, not more feature
widening.

## In Scope

- exercise the generated-compose persistent-data loop in one real project
- prove that task-based seeding and production-pull workflows are adequate on
  the shipped task, exec, workspace binding, and Rhai surfaces
- land any bounded fixes required to make the proof trustworthy
- close `g02.015` if the proof succeeds without exposing another roadmap-sized
  gap

## Out Of Scope

- direct `compose_file` ownership widening
- a new seed-specific product abstraction
- cloud backup or database-aware import/export
- unrelated consumer adoption cleanup outside the proof target

## Acceptance

- one real project uses the shipped generated-compose persistent-data surface
- the proof covers reset retention plus at least the relevant transfer/media or
  pull workflow for that project
- any fixes stay bounded to making the current contract trustworthy
- the lane closes honestly after the proof, or any proof-exposed residue is
  written down explicitly as the only next step

## Outcome

This batch landed through `/Users/tom/Dev/projects/example-app/farmyard`.

What the proof established:

- `farmyard` now uses the shipped generated-compose container path for its
  local Postgres service through manifest-owned `[containers.services]`
- the repo's existing task-owned DB and seed path works against that
  containerized DB through `db:migrate` and `seed:replay:post-sql`
- `effigy container data list` reports the live managed volume on the real
  consumer repo
- `effigy container reset --keep-data` preserves a seeded proof row across a
  full reset/up cycle
- `effigy container data export` and `effigy container data import` restore the
  same proof row after a destructive reset
- `effigy container data pull-production` is adequate for the repo's bounded
  post-SQL replay hook through `scripts/tasks/pull-production-post-sql.sh`

What the proof exposed and fixed in-batch:

- generated top-level compose volume definitions let runtime backends create
  double-prefixed actual volume names, which broke managed-volume reporting and
  transfer on the Colima/nerdctl path
- generated compose output could stay stale when only assembly logic changed,
  because cache reuse only keyed off manifest content and not rendered compose
  drift

Those gaps were fixed in-batch by pinning explicit runtime `name:` on generated
top-level volumes and by forcing compose regeneration whenever the rendered
compose content differs even if the manifest checksum is unchanged.

`g02.015` is now complete on a trustworthy product boundary.

## Next Task

No further execution lives on this lane. Stop in planning and choose which
remaining `g02` lane should resume next.
