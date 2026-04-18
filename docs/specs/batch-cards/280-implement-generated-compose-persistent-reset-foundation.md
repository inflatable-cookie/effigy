# 280 Implement Generated-Compose Persistent Reset Foundation

Status: ready
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Make the first user-visible `g02.015` lifecycle surface real by adding
generated-compose `effigy container reset --keep-data`.

## Context

The catalog and architecture layers already distinguish persistent named
volumes from disposable ones, but the product still only offers all-or-nothing
reset. That leaves one of the main promised data-lifecycle contracts unreal:
developers cannot rebuild their stack while preserving data Effigy already
knows should survive.

## In Scope

- add `effigy container reset --keep-data` to CLI help, parsing, runner
  dispatch, and text/JSON output
- carry generated-compose volume retention metadata through effective
  container policy and report shaping
- preserve persistent named volumes while removing ephemeral volumes on the
  generated-compose reset path
- keep output honest about what was kept and what was removed
- add focused coverage in the affected catalog/container/runner/help surfaces

## Out Of Scope

- `container data list/export/import`
- media bind-mount lifecycle
- `pull_production` hooks or seeding orchestration
- trustworthy `--keep-data` support for direct `compose_file` ownership
- volume migration or backup UX beyond the reset surface

## Acceptance

- generated-compose containers can run `effigy container reset --keep-data`
- persistent named volumes declared through shipped catalog metadata survive
  that reset
- ephemeral volumes still get removed
- direct `compose_file` ownership fails or degrades with an honest bounded
  message instead of pretending Effigy knows which volumes are safe to keep
- focused tests cover parser/help, classification/report shaping, and runner
  behavior

## Next Task

Implement this batch, validate it, then stop in planning for the next bounded
`g02.015` widening step.
