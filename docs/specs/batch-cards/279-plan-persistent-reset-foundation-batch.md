# 279 Plan Persistent Reset Foundation Batch

Status: landed
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Choose the first bounded `g02.015` execution batch now that `g02.016` is
closed.

## Context

The roadmap still describes a broad persistent-data program, but the shipped
substrate is narrower and clearer than that prose implies:

- catalog fragments already carry named-volume and `persist` metadata
- generated compose already assembles those volumes on the product path
- `effigy-catalog::volumes` already knows how to classify reset retention and
  describe Docker volume commands
- the product still lacks any persistent-data lifecycle surface

The first batch should use that substrate directly instead of widening
immediately into import/export or hook orchestration.

## In Scope

- assess the reopened `g02.015` lane against the actual shipped substrate
- choose the smallest trustworthy first execution batch
- make the next planning checkpoint explicit so the lane does not fall back
  into one-card improvisation
- update the strict-lane and front-door planning surfaces

## Out Of Scope

- product execution work
- `container data list/export/import`
- task-owned seeding or `pull_production` hooks
- direct `compose_file` persistent-data ownership

## Acceptance

- one explicit ready execution card exists for `g02.015`
- the chosen batch is bounded to shipped substrate Effigy already owns
- the lane front doors stop leaving `g02.015` as a broad in-progress note

## Result

The first bounded `g02.015` batch should be generated-compose persistent reset
foundation, not the broader data-transfer surface.

Why this batch comes first:

- it closes the immediate product gap where reset still acts like data is
  disposable
- it uses the shipped `persist` metadata and reset classification substrate
  directly
- it keeps the first ownership boundary on generated compose, where Effigy can
  be honest about which volumes are data
- it leaves data import/export and production-pull hooks for a later planning
  checkpoint once the lifecycle foundation is real

The first explicit execution batch is now card `280`.

## Next Task

Execute `280` to land generated-compose `container reset --keep-data` on the
product path.
