# 284 Implement Container Data Transfer Foundation

Status: archived
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Make the next user-visible `g02.015` lifecycle surface real by adding bounded
generated-compose `effigy container data export` and `effigy container data import`.

## Context

`280` made retention real. `282` made inventory real. Operators can now keep
or inspect Effigy-managed volumes, but they still cannot move those volumes
between machines or preserve them outside the local runtime.

The catalog substrate already ships Docker command specs for export/import, so
the next narrow trustworthy widening is transfer, not hook orchestration.

## In Scope

- add `effigy container data export <VOLUME> <PATH>` and
  `effigy container data import <VOLUME> <PATH>` to CLI help, parsing, runner
  dispatch, and text/JSON output
- support bounded transfer for one generated-compose environment by explicit
  managed volume name
- validate that the requested volume belongs to the target Effigy-managed
  environment before invoking runtime transfer
- keep output honest about what file path and managed volume were used
- add focused coverage in the affected catalog/container/runner/help surfaces

## Out Of Scope

- direct `compose_file` ownership
- media bind-mount lifecycle
- cross-project data transfer
- seeding orchestration
- `pull_production` hooks
- backup scheduling or retention policy

## Acceptance

- `effigy container data export <VOLUME> <PATH>` is a real product command
- `effigy container data import <VOLUME> <PATH>` is a real product command
- only volumes owned by the selected generated-compose environment are allowed
- direct `compose_file` ownership fails or degrades with an honest bounded
  message instead of pretending Effigy owns transfer semantics there
- focused tests cover parser/help, managed-volume validation, report shaping,
  and runner behavior

## Result

This batch is now landed.

What changed:

- `effigy container data export` and `effigy container data import` are now
  real CLI/help/parser/runner surfaces on the generated-compose path
- transfer only allows explicitly requested managed volumes that belong to the
  selected Effigy-owned environment
- transfer paths now resolve from the invoking shell while runtime execution
  still runs inside the target repo context
- text and JSON output now report the managed volume plus archive path honestly
- direct `compose_file` ownership now fails explicitly for transfer instead of
  pretending Effigy owns those lifecycle semantics
- focused parser/help, report-shaping, runner-boundary, and CLI integration
  coverage now exists for both transfer commands

## Next Task

No further execution lives on this card. Stop in planning and choose the next
bounded `g02.015` widening step after landed transfer.
