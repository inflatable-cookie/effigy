# 021 - Docs Check Subcommand Consolidation Contract

Status: Active
Owner: Platform
Updated: 2026-05-10

## Purpose

Lock the public command boundary for consolidating the flat `docs check-*`
surface into one `docs check <KIND>` dispatcher before parser or runner
implementation starts.

## Scope

This contract owns:

- the new `docs check <KIND>` command shape
- the bounded `KIND` taxonomy
- migration behavior for removed `check-*` spellings
- the `add-log-index` carveout
- the no-behavior-change rule for the underlying checks

This contract does not own:

- new check kinds
- check implementation behavior changes
- `docs add-log-index` behavior changes

## Command Shape

The consolidated surface is:

```sh
effigy docs check <KIND> [PATHS...] [FLAGS...]
```

`KIND` is one of:

- `links`
- `json-examples`
- `headings`
- `paths`
- `contains`
- `forbidden`
- `index`
- `next-action`
- `workflow-paths`

`effigy docs add-log-index <LOG_FILE>` stays unchanged.

## Migration Rule

Old spellings are removed:

- `docs check-links`
- `docs check-json-examples`
- `docs check-headings`
- `docs check-paths`
- `docs check-contains`
- `docs check-forbidden`
- `docs check-index`
- `docs check-next-action`
- `docs check-workflow-paths`

They must fail with a clear migration error pointing at the replacement:

> `docs check-links` has been replaced by `docs check links`

The same pattern applies to every removed spelling.

## Behavioral Rule

Only the surface changes.

The underlying checks must keep:

- the same arguments
- the same defaults
- the same JSON schemas
- the same check logic

This lane is not allowed to widen into docs-policy behavior changes.

## Parser Boundary

The CLI model collapses the flat docs-check enum variants into one check
variant with a typed kind discriminator.

Minimum boundary:

- `DocsSubcommand::Check { kind, ... }`
- `DocsCheckKind`
- `DocsSubcommand::AddLogIndex { ... }`

## Runner Boundary

Runner dispatch must route all check kinds through one shared dispatcher with
the resolved kind.

The lane should remove caller-local nine-arm branching from the docs runner.

## Acceptance

- all check kinds parse under `docs check <KIND>`
- removed `check-*` spellings fail with migration guidance
- `docs add-log-index` still works unchanged
- visible help/reference output uses the new surface
