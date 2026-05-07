# 509 - Add Data Artifact Handoff Plan Foundation

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Add pure artifact handoff planning for data seed and dump flows.

## Scope

- extend `effigy-data` artifact handoff models where needed
- add pure seed source handoff planning for local versus `oci://` sources
- add pure dump destination handoff planning for local versus `oci://`
  destinations
- preserve runner ownership of actual artifact staging, OCI pull, OCI push, and
  file IO
- add focused `effigy-data` tests for local seed, OCI seed, local dump, planned
  OCI dump, and pushed OCI dump cases

## Non-Goals

- no artifact transport side effects in `effigy-data`
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when data artifact staging/capture intent can be tested
without runner command modules.

## Closeout

Added pure artifact handoff planning in `effigy-data` for local seed sources,
OCI seed sources, local dump destinations, planned OCI dump destinations, and
pushed OCI dump destinations. No artifact transport or file IO moved into the
data crate.

## Validation

- `cargo test -p effigy-data` passed
- `git diff --check` passed

## Next Task

Start card
[`510-wire-data-artifact-handoff-plans-into-runner-glue.md`](./510-wire-data-artifact-handoff-plans-into-runner-glue.md).
