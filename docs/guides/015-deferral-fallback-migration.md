# 015 - Deferral Fallback Migration (Legacy PHP Effigy)

Use this when migrating legacy projects that still rely on the PHP Effigy implementation.

## 1) When to use deferral

Use deferral when:
- the project does not yet have full `effigy.toml` task coverage,
- unresolved requests should be handed off to legacy PHP Effigy,
- you want incremental migration without breaking existing task entrypoints.

## 2) Preferred explicit config

Add to `effigy.toml`:

```toml
[defer]
run = "my-process {request} {args}"
```

Token behavior:
- `{request}`: original task request (`foo`, `catalog-a/test`, etc.)
- `{args}`: passthrough arguments after request
- `{repo}`: shell-quoted catalog/repo path selected for deferral

## 3) Implicit legacy fallback

If no explicit `[defer]` exists, Effigy automatically defers when all are true at workspace root:
- `composer.json` exists
- `effigy.json` exists

Implicit command template:

```bash
<built-in legacy defer process> {request} {args}
```

## 4) Safety guard

Effigy sets `EFFIGY_DEFER_DEPTH` and blocks recursive re-entry after one hop.
If loop detected, execution fails with explicit loop-guard error.

## 5) Migration strategy

1. Start with deferral enabled to preserve behavior.
2. Add first-party tasks in `effigy.toml` incrementally.
3. Keep deferral for unresolved legacy requests during transition.
4. Remove `[defer]` only after critical task paths are represented in `effigy.toml`.

## 6) Deprecation guidance

Treat deferral as a migration bridge, not long-term ownership model.
Recommended deprecation trigger per repo:
- no unresolved requests observed over an agreed validation window,
- primary dev/test/release tasks run through native Effigy tasks,
- fallback invocation is no longer needed in CI or local workflows.
