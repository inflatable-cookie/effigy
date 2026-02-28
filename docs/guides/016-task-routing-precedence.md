# Task Routing Precedence

This guide describes how Effigy routes task requests across catalogs, including alias prefixes, path-based prefixes, and unprefixed fallback.

## Command

Use the built-in diagnostic task:

```bash
effigy tasks
effigy tasks --resolve catalog-a/api
effigy tasks --resolve ../shared/validate
effigy tasks --resolve test
effigy tasks --json
effigy tasks --json --resolve test
effigy tasks --json --pretty false --resolve catalog-a/api
```

The `tasks` output shows discovered catalogs plus routing precedence and resolution evidence.

## Discovery Scope

Catalog discovery walks the workspace tree recursively and includes symlinked directories.

Notes:
- symlinked catalogs are treated the same as physical directories for routing.
- aliases must remain unique across all discovered manifests.
- if duplicate aliases are found (including through symlinked paths), Effigy returns a catalog alias conflict error.

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
    "catalog": "catalog-a",
    "catalog_root": "/abs/path/catalog-a",
    "task": "api",
    "evidence": ["selected catalog via explicit prefix `catalog-a`"],
    "error": null
  }
}
```

Notes:
- `resolve` is `null` when `--resolve` is not passed.
- On resolution failures, `resolve.status` is `error` and `resolve.error` contains the message.
- Built-in selectors (for example `test`) return `resolve.status = "ok"` with `catalog = null` and evidence showing built-in resolution.
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

Effigy resolves in this order:

1. Explicit catalog alias prefix (`catalog-a/api`)
2. Relative or absolute catalog path prefix (`../shared/validate`)
3. Unprefixed nearest in-scope catalog by current working directory
4. Unprefixed shallowest catalog from workspace root

If a prefix can match both an alias and a relative path, alias wins.

## Relative Prefix Notes

- Relative prefixes are resolved from invocation CWD.
- Multi-parent traversal is supported (for example `../../../common/lint`).
- Built-in tasks use the same prefix resolution path as catalog tasks (for example `../shared/tasks`, `../shared/test`).

## Probe Workflow

When routing looks wrong:

1. Run `effigy tasks` and confirm alias/root/depth entries.
2. Run `effigy tasks --resolve <request>` for the exact selector.
3. Check `evidence` lines to confirm whether alias, relative path, CWD-nearest, or shallowest fallback was used.

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
