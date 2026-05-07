# 499 - Wire Manager Runtime Plan Into Generated Image Cleanup

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move generated image cleanup during reset behind a manager-owned runtime
invocation plan.

## Scope

- add a small manager-owned runtime invocation plan shape if needed
- wire generated image `docker image rm -f` cleanup through the manager plan
- preserve missing-image tolerance and error rendering

## Non-Goals

- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when generated image cleanup no longer constructs runtime
process invocation locally.

## Closeout

Generated image cleanup now uses a manager-owned runtime invocation plan before
executing `image rm -f`.

Missing-image tolerance and error rendering are unchanged.

## Validation

- `cargo test -p effigy-container-manager`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Run final g04.004 drift review and decide closeout.
