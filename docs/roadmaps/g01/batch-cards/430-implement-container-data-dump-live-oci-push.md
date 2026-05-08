# 430 - Implement Container Data Dump Live OCI Push

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-07

## Goal

Allow `container data dump <TARGET>=oci://<REF> --push` to publish a captured
SQL dump as an OCI artifact.

## Scope

- add an explicit `--push` flag to `container data dump`
- keep local dump behavior unchanged
- reject local-only dumps when `--push` is supplied
- route OCI dump publication through the existing artifact capture push path
- report pushed artifact metadata through the existing JSON payload
- update help, contract, changelog, and focused parser/help tests

## Non-Goals

- no implicit registry mutation
- no overwrite flag
- no credential manager
- no Acowtancy app migration changes
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when `container data dump --db-dump <TARGET>=oci://<REF>
--push` stages the SQL dump locally, publishes it through the artifact adapter,
and reports the pushed digest through `artifact_capture.destination`.

## Closeout

- `ContainerDataSubcommand::Dump` now carries explicit push intent.
- `container data dump --push` is accepted by the parser.
- local-only dump specs reject `--push`.
- OCI dump destinations pass `push=true` to the artifact capture path.
- help, contract, and changelog document the opt-in write behavior.

## Next Task

No active ready card.

Stop in planning and choose the next roadmap deliberately.
