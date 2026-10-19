# 046 Task Visibility And Grouping Contract

Owner: task manifest, discovery, routing, and execution maintainers
Architecture: [`028`](../architecture/028-published-and-draft-task-surfaces.md)

## Manifest grammar

All runnable definitions belong to `[tasks]`. Every supported task shorthand,
full task table, bundle export and included fragment may declare:

```toml
[tasks.build]
run = "./scripts/build"
visibility = "public"
group = "Development"

[tasks."build:prepare"]
run = "./scripts/prepare"
visibility = "internal"
```

`visibility` accepts only `public` and `internal`. `group` is an optional,
non-empty string. Visibility controls discovery, never execution permission or
secrecy. Public tasks may call internal tasks and internal tasks may call public
tasks. Both use the ordinary resolver, canonical TaskId, kernel gates, run
journal and execution pipeline. Visibility and grouping do not change identity.

## Effective defaults

The planner's task-visibility ruling (2026-10-19, effigy#228) replaces the former
provisional task namespace. Explicit visibility always wins, with reason
`declared`. Otherwise:

1. A task with an unnamespaced name (no `:` in its local task name) is a public
   top-level entry point, even when another task references it.
2. A namespaced task referenced by another task is internal, with reason
   `referenced-only`.
3. Every other task is public, with reason `default`.

References include typed sequence steps, `task:` steps, concurrent tasks and
setup steps, and all managed profiles. They resolve through the ordinary
resolver; bare references are pinned to the declaring catalog, while qualified
references may cross catalogs. Missing or ambiguous references hide nothing.

The unnamespaced entry-point rule is a consumer-proof refinement: treating every
reference as internal hid `cream/build`, `cream/dev`, `validate` and other normal
Acowtancy entry points. This rule depends on task grammar, not consumer names or
paths. Authors can explicitly hide an unnamespaced helper or publish a
namespaced entry point. Standalone tasks remain public unless authors opt out;
reference inference cannot identify every generated proof task.

Effigy's own focused test, compile and proof selectors explicitly declare
internal visibility. Its main development, validation and release selectors
are public and grouped. Consumers need no manifest migration to keep running.

## Inventory and completion

- `effigy tasks` lists public tasks, grouped by `group`. Ungrouped rows use the
  `General` section. Managed profile rows inherit their parent task's visibility
  and group. Catalog and built-in inventory remain available.
- `effigy tasks --all` includes internal tasks in the same grouped text view.
- `effigy tasks --json` always includes every task, independently of `--all`.
  Existing `effigy.tasks.v1` and filtered payloads gain additive `visibility`,
  `visibility_reason` and `group` fields on each catalog task row. Group is
  `null` when undeclared; empty-catalog placeholder rows have null metadata.
  `selectors` remains the complete machine inventory.
- Filtering with `--task` preserves the selected output's visibility policy;
  `--resolve` and direct invocation resolve internal tasks normally.
- Shell completion offers public tasks only. Its cache observes composed task
  fragments and bundle source files so metadata edits invalidate candidates.
- Help directs users to the grouped public inventory and the full agent view.

`tasks status --all` is a separate status query and retains its existing scope;
the listing `--all` flag never changes execution or status identity.

## Built-in migration from v0.14.1

For one release, the manifest decoder translates `[drafts.<name>]` into
`[tasks.<name>]` with explicit internal visibility. It removes the old `created`,
`expires` and `purpose` lifecycle fields and translates `{ draft = "..." }`
steps into ordinary task references, including managed setup/profile steps.
A definition declared in both namespaces fails with an actionable rename error;
it is never silently overwritten. Includes and bundles use the same decoder.
The translation is in memory and never edits a consumer checkout.

Legacy QA member `surface = "draft"` translates to `published`, the single
runtime surface for public and internal tasks. QA group lifecycle dates remain
supported independently of task visibility.

The decoder warns with the replacement syntax. `effigy draft <name>` warns and
runs the ordinary task; `effigy drafts` refuses with `effigy tasks --all` and
`effigy tasks --json` as replacements. The old commands are absent from general
help and completion; their dedicated help panels explain migration.

The separate draft task model, inventory, expiry check, selection namespace,
step executor and draft run-payload additions are removed. Existing historical
surface discriminators remain readable as historical identities; migrated new
runs always use the ordinary task identity. No draft-specific mutable state or
parallel writer remains. No files are implicitly discovered, pruned or deleted.
