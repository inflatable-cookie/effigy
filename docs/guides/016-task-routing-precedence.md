# Task Routing Precedence

This guide describes how Effigy routes task requests across catalogs, including alias prefixes, path-based prefixes, and unprefixed fallback.

## Command

Use the built-in diagnostic task:

```bash
effigy catalogs
effigy catalogs --resolve farmyard/api
effigy catalogs --resolve ../froyo/validate
effigy catalogs --json
effigy catalogs --json --pretty false --resolve farmyard/api
```

The `catalogs` output shows discovered catalogs plus routing precedence and resolution evidence.

## JSON Output

Use JSON output for automation, reports, or diffing:

```bash
effigy catalogs --json
effigy catalogs --json --resolve farmyard/api
effigy catalogs --json --pretty false --resolve ../froyo/validate
```

Schema (stable keys):

```json
{
  "catalogs": [
    {
      "alias": "farmyard",
      "root": "/abs/path/farmyard",
      "depth": 1,
      "manifest": "/abs/path/farmyard/effigy.toml",
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
    "selector": "farmyard/api",
    "status": "ok",
    "catalog": "farmyard",
    "catalog_root": "/abs/path/farmyard",
    "task": "api",
    "evidence": ["selected catalog via explicit prefix `farmyard`"],
    "error": null
  }
}
```

Notes:
- `resolve` is `null` when `--resolve` is not passed.
- On resolution failures, `resolve.status` is `error` and `resolve.error` contains the message.
- `--pretty false` emits compact one-line JSON.

## Precedence Order

Effigy resolves in this order:

1. Explicit catalog alias prefix (`farmyard/api`)
2. Relative or absolute catalog path prefix (`../froyo/validate`)
3. Unprefixed nearest in-scope catalog by current working directory
4. Unprefixed shallowest catalog from workspace root

If a prefix can match both an alias and a relative path, alias wins.

## Relative Prefix Notes

- Relative prefixes are resolved from invocation CWD.
- Multi-parent traversal is supported (for example `../../../shared/lint`).
- Built-in tasks use the same prefix resolution path as catalog tasks (for example `../froyo/tasks`, `../froyo/test`).

## Probe Workflow

When routing looks wrong:

1. Run `effigy catalogs` and confirm alias/root/depth entries.
2. Run `effigy catalogs --resolve <request>` for the exact selector.
3. Check `evidence` lines to confirm whether alias, relative path, CWD-nearest, or shallowest fallback was used.

## Report Diff Example

Capture snapshots before and after a routing change:

```bash
effigy catalogs --json > reports/catalogs-before.json
effigy catalogs --json --resolve ../froyo/validate > reports/catalogs-probe-before.json

# apply changes

effigy catalogs --json > reports/catalogs-after.json
effigy catalogs --json --resolve ../froyo/validate > reports/catalogs-probe-after.json
```

Then diff:

```bash
diff -u reports/catalogs-before.json reports/catalogs-after.json
diff -u reports/catalogs-probe-before.json reports/catalogs-probe-after.json
```
