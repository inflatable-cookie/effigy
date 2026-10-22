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
  `visibility_reason`, `group` and `description` fields on each catalog task
  row. Group and description are `null` when undeclared; empty-catalog
  placeholder rows have null metadata. `selectors` remains the complete
  machine inventory.
- A task definition may declare one `description` line (200 characters
  maximum, non-empty, single line) on every task shape, including
  bundle-provided tasks. `effigy tasks` prints it under the task in its group.
  `effigy <task> --help` answers from the declaration — selector, group,
  description, run preview and manifest — before any run, gate or managed
  supervisor starts, and emits the built-in `effigy.help.v1` payload under
  `--json`. A `--help` after the invocation's `--` is still the task's own
  argument and is passed through.
- Filtering with `--task` preserves the selected output's visibility policy;
  `--resolve` and direct invocation resolve internal tasks normally.
- Shell completion offers public tasks only. Its cache observes composed task
  fragments and bundle source files so metadata edits invalidate candidates.
- General help (`--help`, `help`) and `help work` include the same grouped public
  task inventory. Other help topics remain scoped to their command. Invalid
  manifests leave built-in help available with a task discovery warning.

`tasks status --all` is a separate status query and retains its existing scope;
the listing `--all` flag never changes execution or status identity.

## Retired draft namespace (v0.15)

The draft task namespace is retired (Q-009). `[drafts]`, `draft` step
references, QA member `surface = "draft"` and the `effigy draft`/`effigy
drafts` commands fail with an actionable error that names `[tasks.<name>]`
with `visibility = "internal"`, `task = "<name>"` and `effigy tasks --all`.
Nothing is translated, warned or ignored, and no consumer checkout is edited.

A legacy definition moves to ordinary tasks:

```toml
[tasks."proof:smoke"]
run = "printf smoke"
visibility = "internal"
```

```toml
[tasks.entry]
run = [{ task = "proof:smoke" }]
```

The `created`, `expires` and `purpose` lifecycle fields are not part of the
task grammar; remove them. A definition that used to be a draft is now an
internal task with the ordinary task identity, resolver, gate, run journal and
payload. Existing historical surface discriminators remain readable as
historical identities; new runs always use the ordinary task identity. No
files are implicitly discovered, pruned or deleted.

