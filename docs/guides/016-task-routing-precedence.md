# Task Routing Precedence

This guide describes how Effigy routes task requests across catalogs, including alias prefixes, path-based prefixes, and unprefixed fallback.


## Vision Alignment

- Primary tags: `ROUTE`, `OPERATE`
- Target movement: routing precedence and resolution evidence remain deterministic and fast to diagnose.

## Command

Use the built-in diagnostic task:

```bash
effigy tasks
effigy tasks --resolve catalog-a/api
effigy --plan catalog-a/api
effigy --json --plan catalog-a/api
effigy tasks --resolve ../shared/validate
effigy tasks --resolve test
effigy tasks --json
effigy tasks --json --resolve test
effigy tasks --json --pretty false --resolve catalog-a/api
```

The `tasks` output shows effective catalogs plus routing precedence and resolution evidence.

One resolver decides where a selector goes. Running it, `--plan` and
`tasks --resolve` all ask that resolver, so the probe always says what running
the selector would do.

`effigy --plan <selector>` is the non-executing command-shape path. It is not
`tasks --resolve` and it is not `--json`. `--json` only chooses the envelope;
`effigy --json <selector>` still runs the task. An unresolved selector with
`--plan` fails without deferral or exec-alias fallback; `effigy --plan test`
keeps the test builtin.

## Effigy flags come before the task

```bash
effigy [--json] [--plan] [--repo <PATH>] [--lock-wait-ms <MS>] [--env-schema <PATH>] [--verbose-root] <task> [task args]
```

Everything after the task name belongs to the task: `effigy lint --json`
passes `--json` to the linter, and `effigy gh:sync --repo owner/x` passes
`--repo owner/x` to the task. For one release, `--plan` and `--json` directly
after the task name are still read as Effigy's, with a note naming the leading
spelling. Built-in commands routed by name (`test`, `tasks`, ...)
keep their own flag positions.

## Built-in Names And Repository Tasks

A repository task owns a word a built-in also uses: `effigy <name>` runs the
task, and everything after the task name goes to it. That is why a repository
with `fmt = "cargo fmt {args}"` keeps `effigy fmt`, `effigy fmt --check` and
`effigy fmt rust <FILE>` as its own task invocations even though Effigy ships a
built-in Rust formatter under that word.

The exceptions are deliberate: `test` is Effigy's orchestration front door
(contract [038](../knowledge/contracts/038-unified-test-orchestration-contract.md)),
and `help` and `version` answer questions about Effigy itself. A task cannot
shadow those three.

A built-in that a task shadows may keep an explicit spelling of its own. When
that exists, `effigy --plan <name>` says so:

```bash
effigy --plan fmt
# … Built-in shadowed: `fmt` runs this repository's task; `effigy fmt-rust`
# reaches the built-in with the same arguments
```

`effigy fmt-rust <FILE>...` is the built-in Rust formatter's explicit spelling,
with the same `--check`, `--repo` and `--json` flags. Every shadowable built-in
is classified in `effigy-cli`'s `builtin_names` table, so a new built-in cannot
quietly take a consumer task name.

## Root And Membership

Catalog membership is explicit ([037](../knowledge/contracts/037-explicit-catalog-membership-contract.md)):
the root manifest's `[catalog.members]` names its catalogs; nothing walks the
tree for them.

Running from inside a member catalog, however deep, resolves to the root that
declares it, so the full catalog set, the same task identities and the same
run history apply wherever you stand. The walk stops at the Git top level. A
catalog no ancestor declares stays its own root.

Notes:
- aliases must remain unique across the effective catalogs.

## JSON Output

Use JSON output for automation, reports, or diffing:

```bash
effigy tasks --json
effigy tasks --json --resolve catalog-a/api
effigy tasks --json --resolve test
effigy tasks --json --pretty false --resolve ../shared/validate
```

Schema (stable keys):

```json
{
  "catalogs": [
    {
      "alias": "catalog-a",
      "root": "/abs/path/catalog-a",
      "depth": 1,
      "manifest": "/abs/path/catalog-a/effigy.toml",
      "has_defer": false
    }
  ],
  "precedence": [
    "explicit catalog alias prefix",
    "relative/absolute catalog path prefix",
    "unprefixed nearest in-scope catalog by cwd",
    "unprefixed shallowest catalog from workspace root"
  ],
  "resolve": {
    "selector": "catalog-a/api",
    "status": "ok",
    "route": "task",
    "catalog": "catalog-a",
    "catalog_root": "/abs/path/catalog-a",
    "task": "api",
    "task_id": {
      "root": "/abs/path",
      "catalog": "catalog-a",
      "task": "api",
      "selector": "catalog-a/api"
    },
    "lock_scopes": ["task:catalog-a/api"],
    "evidence": ["selected catalog via explicit prefix `catalog-a`"],
    "error": null
  }
}
```

Notes:
- `resolve` is `null` when `--resolve` is not passed.
- `resolve.route` is where the selector goes: `typed-command` (a built-in
  command such as `release`, which runs before a member catalog's task of that
  name; a root task of that name owns the word),
  `task`, `builtin` (`test`, `tasks`, ...), `exec-alias`, `defer` or `error`.
- On resolution failures, `resolve.status` is `error` and `resolve.error` contains the message.
- `task_id` is the identity a run of that task uses for its lock and status.
- `--pretty false` emits compact one-line JSON.

## CI-safe JSON Assertions

Capture clean JSON payload:

```bash
effigy --json tasks --resolve catalog-a/api > /tmp/effigy-tasks.json
```

Assert probe success:

```bash
jq -e '.resolve.status == "ok"' /tmp/effigy-tasks.json >/dev/null
jq -e '.resolve.catalog == "catalog-a"' /tmp/effigy-tasks.json >/dev/null
jq -e '.resolve.task == "api"' /tmp/effigy-tasks.json >/dev/null
```

Assert top-level shape:

```bash
jq -e 'has("catalogs") and has("precedence") and has("resolve")' /tmp/effigy-tasks.json >/dev/null
jq -e '(.catalogs | type) == "array"' /tmp/effigy-tasks.json >/dev/null
jq -e '(.precedence | length) == 4' /tmp/effigy-tasks.json >/dev/null
```

Compact mode fixture capture:

```bash
effigy --json tasks --pretty false --resolve catalog-a/api > /tmp/effigy-tasks-compact.json
```

## Precedence Order

A typed built-in command (`release`, `deploy`, `container`, ...) wins first,
unless the root lists it in `[defer].builtins` or declares a root task of that
name. Otherwise a task is selected:

1. Explicit catalog alias prefix (`catalog-a/api`)
2. Relative or absolute catalog path prefix (`../shared/validate`)
3. Unprefixed nearest in-scope catalog by current working directory
4. Unprefixed shallowest catalog from workspace root

If a prefix can match both an alias and a relative path, alias wins. Only when
no task matches does a routed built-in (`test`, `tasks`, ...) run, then an exec
alias, then the `[defer]` command (deferral before exec aliases inside a
container handoff).

## Task visibility

Public and internal tasks use the same routing and runtime. `effigy tasks`
shows public tasks by group; `effigy tasks --all` and `effigy tasks --json`
include internal tasks. Run an internal task directly with `effigy <selector>`.
`--resolve` and `--plan` can inspect any task.

Declare `visibility = "public" | "internal"` and optional `group` on a task.
Unnamespaced entry points remain public; referenced namespaced helpers default
to internal. See [contract 046](../knowledge/contracts/046-published-and-draft-task-surface-contract.md)
for defaults and the one-release legacy syntax translation.

## Relative Prefix Notes

- Relative prefixes are resolved from invocation CWD.
- Multi-parent traversal is supported (for example `../../../common/lint`).
- Built-in tasks use the same prefix resolution path as catalog tasks (for example `../shared/tasks`, `../shared/test`).

## Probe Workflow

When routing looks wrong:

1. Run `effigy tasks` and confirm alias/root/depth entries.
2. Run `effigy tasks --resolve <request>` for the exact selector.
3. Check `evidence` lines to confirm whether alias, relative path, CWD-nearest, or shallowest fallback was used.

## Child Task Ownership Rule

Do not duplicate a child-catalog task at the workspace root just to make it
callable from the root.

If one discovered child catalog is the only owner of a task name, Effigy can
resolve that unprefixed task from the workspace root already.

Example:

- child catalog owns `catalog_a/db:reset`
- no other catalog defines `db:reset`
- from workspace root, `effigy db:reset` resolves to `catalog_a/db:reset`

Recommended policy:

- keep the task only in the catalog that actually owns the behavior
- add a root task only when the root owns different orchestration behavior
  (for example a multi-step aggregate such as `validate` or `qa`)
- if multiple catalogs define the same task, require an explicit prefix instead
  of adding a duplicate root shim

Why this matters:

- avoids root-manifest duplication and drift
- keeps task ownership close to the code it operates on
- lets agents and contributors trust routing rather than cargo-culting root aliases

## Report Diff Example

Capture snapshots before and after a routing change:

```bash
effigy tasks --json > reports/tasks-before.json
effigy tasks --json --resolve ../shared/validate > reports/tasks-probe-before.json

# apply changes

effigy tasks --json > reports/tasks-after.json
effigy tasks --json --resolve ../shared/validate > reports/tasks-probe-after.json
```

Then diff:

```bash
diff -u reports/tasks-before.json reports/tasks-after.json
diff -u reports/tasks-probe-before.json reports/tasks-probe-after.json
```

## Related Guides

- [`018-doctor-explain-mode.md`](018-doctor-explain-mode.md)
- [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)

## Next Step

After changing routing behavior, update probe snapshots and run the docs QA flow in [`029-docs-qa-checklist-and-validation.md`](029-docs-qa-checklist-and-validation.md).
