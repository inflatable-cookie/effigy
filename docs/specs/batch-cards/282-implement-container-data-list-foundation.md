# 282 Implement Container Data List Foundation

Status: ready
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Make the next user-visible `g02.015` surface real by adding bounded
`effigy container data list`.

## Context

Generated-compose `reset --keep-data` now preserves persistent volumes, but
operators still cannot inspect what managed data volumes exist from the product
surface. That leaves the data lifecycle story half-blind.

## In Scope

- add `effigy container data list` to CLI help, parsing, runner dispatch, and
  text/JSON output
- list managed named volumes for one container environment with honest
  persist/ephemeral classification and any available runtime size metadata
- keep the first batch bounded to product-owned generated-compose metadata
  and direct runtime inspection where Effigy can be trustworthy
- add focused coverage in the affected catalog/container/runner/help surfaces

## Out Of Scope

- export/import
- media bind-mount lifecycle
- `pull_production` hooks or seeding orchestration
- cross-project data inventory
- broader backup UX

## Acceptance

- `effigy container data list` is a real product command
- generated-compose environments report their managed named volumes with
  honest persist/ephemeral classification
- output stays clear when runtime size metadata is unavailable
- focused tests cover parser/help, policy/report shaping, and runner behavior

## Next Task

Implement this batch, validate it, then stop in planning for the next bounded
`g02.015` widening step.
