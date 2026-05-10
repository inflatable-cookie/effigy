# 023 - Docs Check Subcommand Consolidation

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-10
Depends on:
- [`022-remote-bundle-sources-git-and-oci-delivery.md`](./022-remote-bundle-sources-git-and-oci-delivery.md)

## Goal

Collapse the 10 `docs check-*` subcommands into a single `docs check <KIND>`
surface. Keep `docs add-log-index` unchanged.

## Scope

- Replace individual subcommands with one dispatcher:
  ```
  effigy docs check links [PATHS...]
  effigy docs check json-examples [PATHS...] --file --section --min-blocks --require --require-block
  effigy docs check headings [PATHS...] --require-heading
  effigy docs check paths [PATHS...]
  effigy docs check contains [PATHS...] --require
  effigy docs check forbidden [PATHS...] --forbid
  effigy docs check index --policy-index --dir --index
  effigy docs check next-action --policy
  effigy docs check workflow-paths --dir
  ```
- `docs add-log-index <LOG_FILE>` remains as-is.
- Update CLI parser: replace `DocsSubcommand` enum variants with a single
  `Check { kind: DocsCheckKind, ... }` variant.
- Update runner: route through existing `checks.rs` dispatcher using the kind
  parameter.
- Update `docs/guides/025-command-reference-matrix.md`.
- Update any shell scripts or CI workflows using the old spellings.
- Update completion generators.

## Non-Goals

- No changes to check logic or behavior (only the surface changes)
- No changes to `docs add-log-index`
- No `.github/workflows/` edits
- No release execution

## Why Now

The `docs` command has 10 subcommands that all share the same dispatch pattern
and route to the same internal module. This is the broadest flat subcommand
surface in Effigy. Consolidating into `check <KIND>` aligns with how other
commands use positional arguments for operation selection (`deploy export
<PROVIDER>`, `artifact stage <REF>`).

## Core Decisions

### New Surface

```
effigy docs check <KIND> [PATHS...] [FLAGS...]
```

`KIND` is one of: `links`, `json-examples`, `headings`, `paths`, `contains`,
`forbidden`, `index`, `next-action`, `workflow-paths`.

Each kind retains its existing flags. The positional `PATHS...` argument stays
optional and defaults to the same behavior as today.

### Backward Compatibility

None. Old spellings (`docs check-links`, `docs check-paths`, etc.) will fail
with a clear error: "`docs check-links` has been replaced by `docs check links`".

### Internal Routing

`docs_command/mod.rs` currently has a 10-arm match. After consolidation it
becomes:

```rust
match subcommand {
    DocsSubcommand::Check { kind, paths, .. } => {
        checks::run(kind, paths, ...)
    }
    DocsSubcommand::AddLogIndex { file } => {
        checks::add_log_index(file)
    }
}
```

## Success Criteria

- All 9 check kinds work under `docs check <KIND>`
- Old `docs check-*` spellings produce a clear migration error
- `docs add-log-index` continues to work unchanged
- Reference guide updated
- Completion scripts updated
- Tests converted to new spelling
- Changelog entry under `[Unreleased] Breaking`

## Suggested Batch Order

1. Update CLI parser (`crates/effigy-cli/`)
2. Update runner dispatch (`src/runner/docs_command/`)
3. Update reference guide
4. Update tests
5. Update completions

## Validation

- Each check kind executes correctly
- Old spelling produces error with migration hint
- `git diff --check`
- docs path/link checks

## Next Task

Execute
[`639-close-docs-check-runner-docs-and-completions.md`](./batch-cards/639-close-docs-check-runner-docs-and-completions.md).
