# 639 - Close Docs Check Runner, Docs, And Completions

Lane: [`066-docs-check-subcommand-consolidation-strict-lane.md`](../066-docs-check-subcommand-consolidation-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Goal

Finish the `docs check` consolidation across the remaining operator-facing
surfaces and close `g04.023`.

## Scope

- update broad docs/help/reference output to the new `docs check <KIND>` forms
- update completion surfaces
- update runner and CLI output tests that still assert the old spellings
- close the lane once the focused docs-command proof round is green

## Acceptance

- visible docs/help no longer recommend `docs check-*`
- completion surfaces emit `docs check <KIND>`
- focused docs parser/runner/help/CLI tests are green
- `g04.023` can be marked complete

## Next Task

Close the remaining runner/docs/completion slice and finish the lane.
