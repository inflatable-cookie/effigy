# 513 - Close Data Pipeline Foundation Pass

Lane: [`047-data-seed-dump-pipeline-strict-lane.md`](../047-data-seed-dump-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Close the first `g04.005` foundation pass and select the next larger migration
shape.

## Scope

- summarize what moved into `effigy-data`
- inventory remaining runner ownership after cards `503` through `512`
- decide whether next work is manifest adapter target resolution or runner file
  split
- create the next ready card

## Non-Goals

- no code migration in this card
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when the foundation pass is closed and the next bounded
`g04.005` card is ready.

## Foundation Pass Summary

Cards `503` through `512` established `effigy-data` as the dependency-light
planning crate for data seed/dump flows.

Moved into `effigy-data`:

- seed/dump target identity models
- postgres/mariadb seed and dump command rendering
- `oci://` data artifact reference classification
- seed source path normalization
- dump destination path normalization
- local/OCI artifact handoff planning
- DB seed artifact staging path/root planning

Remaining runner ownership:

- manifest-backed logical target collection from `[bundle].databases` and
  `[data.targets]`
- seed target validation and duplicate target checks
- dump target validation and duplicate target/path checks
- database service collection and matching from container manifest config
- artifact parsing, local file checks, OCI pull, local/OCI staging, OCI capture
  and push
- prompts, task dispatch, container exec, file IO, and output rendering

## Decision

Next work should be a manifest adapter target-resolution slice, not a file split
yet. Splitting files before target resolution moves would only distribute the
same ownership problem across more modules.

## Validation

- `git diff --check` passed

## Next Task

Start card
[`514-add-data-target-manifest-adapter-foundation.md`](./514-add-data-target-manifest-adapter-foundation.md).
