# 015 - Deferral Migration (Legacy PHP Effigy)

Use this when migrating legacy projects that still rely on the PHP Effigy
implementation.

## 1) When to use deferral

Use deferral when:
- the project does not yet have full `effigy.toml` task coverage,
- unresolved requests should be handed off to legacy PHP Effigy,
- you want incremental migration without breaking existing task entrypoints.

Use explicit `effigy defer ...` when:
- you already know you want the legacy fallback path,
- the request name is known up front (`prep`, `release`, `seed`, etc.),
- you want to bypass normal selector resolution instead of waiting for a miss.

Use normal task invocation when:
- you want Effigy to try first-party `[tasks]`, built-ins, and routing first,
- deferral should only happen if nothing matches locally.

## 2) Preferred explicit config

Add to `effigy.toml`:

```toml
[defer]
run = "my-process {request} {args}"
builtins = ["release"]
run_in = "host" # or "container" / "either"
```

Legacy PHP example:

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
builtins = ["release"]
run_in = "container"
```

Token behavior:
- `{request}`: original task request (`foo`, `catalog-a/test`, etc.)
- `{args}`: passthrough arguments after request
- `{repo}`: shell-quoted catalog/repo path selected for deferral

Runtime behavior:
- omitted `run_in` stays host-only for backward compatibility,
- `run_in = "container"` reuses the normal container/runtime path,
- `run_in = "either"` prefers container binding when a default target exists and
  otherwise falls back to host execution.
- deferred container requests now share the same non-shell activation contract
  as explicit `run_in = "container"` tasks:
  - runtime auto-start when needed
  - sibling-service and exec-readiness prep before dispatch
  - temporary host-container lease refresh for auto-started or already-leased
    runtimes
  - public gateway/route reconciliation for containers that declare a gateway
    surface

Lease behavior:
- default timeout is 5 minutes
- reuse refreshes the lease
- the reaper shuts the runtime down after the lease expires unless another
  owned session or explicit `effigy container up` keeps it alive

Optional built-in bypass:
- `builtins = ["release", ...]` tells Effigy to skip its own parser-level built-in for those command families and treat them like deferred legacy requests instead
- use this for legacy repos where commands such as `release` already exist in the old system and must not be shadowed by Effigy's native built-ins
- explicitly deferred built-ins are also removed from general help and from the built-in section in `effigy tasks`

## 3) Explicit command surface

Run the deferral surface directly when you want the configured fallback without
waiting for selector resolution to miss:

```bash
effigy defer prep
effigy defer release -- --dry-run
effigy defer --repo /path/to/legacy-site seed
```

This uses the same `[defer]` contract automatic fallback uses:
- same command template,
- same `run_in` handling,
- same built-in bypass behavior,
- same loop guard.

## 4) No implicit legacy fallback

Effigy no longer enables deferral just because a repo has:
- `composer.json`
- `effigy.json`

Legacy repos now need an explicit `[defer]` block or a bundle source that
provides one.

That means:
- legacy markers alone no longer hide `release`,
- legacy markers alone no longer route unresolved selectors through Composer-global Effigy,
- migration repos should make deferral ownership explicit in `effigy.toml`.

## 5) Safety guard

Effigy sets `EFFIGY_DEFER_DEPTH` and blocks recursive re-entry after one hop.
If loop detected, execution fails with explicit loop-guard error.

## 6) Migration strategy

1. Start with deferral enabled to preserve behavior.
2. Add first-party tasks in `effigy.toml` incrementally.
3. Keep automatic selector-miss deferral for unresolved legacy requests during
   transition.
4. Use explicit `effigy defer ...` for high-value legacy commands you still
   want available even after first-party tasks start landing.
5. Remove `[defer]` only after critical task paths are represented in `effigy.toml`.

## 7) Deprecation guidance

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

After deferral is stable, migrate one high-volume selector path into explicit
`[tasks]` and validate it with `effigy tasks --resolve`.
