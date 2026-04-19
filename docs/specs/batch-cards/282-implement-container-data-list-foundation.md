# 282 Implement Container Data List Foundation

Status: landed
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

## Result

This batch is now landed.

What changed:

- `effigy container data list` is now a real CLI/help/parser/runner surface on
  the generated-compose path
- the container runner now hydrates managed-volume inventory with best-effort
  runtime size and mount metadata when the active runtime can provide it
- text and JSON output now report persistent-vs-ephemeral classification
  honestly while degrading cleanly when runtime metadata is unavailable
- direct `compose_file` ownership now fails explicitly for `data list` instead
  of claiming trustworthy volume inventory Effigy does not own yet
- focused parser/help, report-shaping, runner, and CLI integration coverage now
  exists for the new product surface

## Next Task

No further execution lives on this card. Stop in planning and decide the next
bounded `g02.015` widening step after inventory.
