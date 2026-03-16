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
builtins = ["release"]
```

Token behavior:
- `{request}`: original task request (`foo`, `catalog-a/test`, etc.)
- `{args}`: passthrough arguments after request
- `{repo}`: shell-quoted catalog/repo path selected for deferral

Optional built-in bypass:
- `builtins = ["release", ...]` tells Effigy to skip its own parser-level built-in for those command families and treat them like deferred legacy requests instead
- use this for legacy repos where commands such as `release` already exist in the old system and must not be shadowed by Effigy's native built-ins
- explicitly deferred built-ins are also removed from general help and from the built-in section in `effigy tasks`

## 3) Implicit legacy fallback

If no explicit `[defer]` exists, Effigy automatically defers when all are true at workspace root:
- `composer.json` exists
- `effigy.json` exists

Implicit command template:

```bash
<built-in legacy defer process> {request} {args}
```

Implicit built-in bypass:
- in that automatic PHP-legacy mode, `release` is deferred by default even without `builtins = ["release"]`
- that also hides `release` from general help and from the built-in section in `effigy tasks`
- add explicit `builtins = [...]` only when you need to bypass additional native built-ins beyond that default

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

## Related Guides

- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

## Next Step

After deferral is stable, migrate one high-volume selector path into explicit `[tasks]` and validate it with `effigy tasks --resolve`.
