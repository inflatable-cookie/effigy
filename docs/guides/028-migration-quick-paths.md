# 028 - Migration Quick Paths

Use this guide to choose the shortest safe migration path for common Effigy adoption scenarios.

For detailed command syntax, see [`025-command-reference-matrix.md`](./025-command-reference-matrix.md).
For CI implementation templates, see [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md).

## 1) Path A: New Repo Onboarding

When to use:
- no existing `effigy.toml`
- team wants quick task standardization

Decision path:
1. Create baseline manifest.
2. Add minimal tasks (`dev`, `lint`, `build`, `validate`) and keep `test` on
   the built-in runner unless you need an explicit override.
3. Verify routing and health.
4. Add CI JSON checks only after local task flow is stable.

Fast commands:

```sh
effigy init
effigy tasks
effigy doctor --verbose
effigy test --plan
```

Starter manifest:

```toml
[catalog]
alias = "app"

[package_manager]
js = "bun"

[tasks]
dev = "bun run dev"
lint = "bun run lint"
build = "bun run build"
validate = [{ task = "lint" }, { task = "build" }]
```

Exit criteria:
- `effigy tasks` lists expected tasks
- `effigy doctor` has no blocking errors
- `effigy test --plan` shows expected runner/suite routing

## 2) Path B: Legacy Deferral Cleanup

When to use:
- unresolved selectors rely on `[defer]`
- legacy runner still handles parts of task surface

Decision path:
1. Inventory unresolved selector flow.
2. Promote frequently-used deferred selectors into explicit `[tasks]` entries.
3. Keep `[defer]` as fallback for low-volume legacy paths.
4. Remove `[defer]` only after no critical selector depends on it.

Fast commands:

```sh
effigy tasks --resolve <selector>
effigy doctor <selector> -- <args>
```

Temporary compatibility snippet:

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
builtins = ["release"]
```

Use `builtins = ["release"]` when a legacy command family still owns that name and Effigy's native built-in would otherwise intercept it during migration.

Exit criteria:
- high-frequency selectors resolve directly via catalogs
- no deferral loop errors
- `[defer]` usage is intentional and documented

## 3) Path C: CI JSON Adoption

When to use:
- CI currently parses text output
- contracts and machine payload stability are required

Decision path:
1. Switch automation to `effigy --json <command>`.
2. Add contract checks in PR path.
3. Validate selection artifact payload.
4. Upload triage artifacts for failures.

Core scripts:

```sh
effigy contracts check-json --full --print-selected=json
effigy contracts validate-selection --artifact ./json-contracts-selected.json
```

Exit criteria:
- CI no longer depends on human-rendered text parsing
- contract validation job passes on PR and main
- triage artifacts are uploaded on failure

Implementation details live in [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md).

## 4) Path D: Monorepo Expansion (Single Catalog -> Multi-Catalog)

When to use:
- repo now has multiple independently-owned subprojects

Decision path:
1. Split child manifests by ownership boundary.
2. Assign unique `[catalog].alias` per child.
3. Keep root manifest for orchestration-only tasks.
4. Prefer prefixed invocation in shared scripts (`<catalog>/<task>`).

Fast commands:

```sh
effigy tasks
effigy tasks --resolve api/validate
effigy tasks --resolve web/validate
```

Exit criteria:
- no alias conflicts
- no ambiguous shared selectors in CI-critical paths
- root orchestration tasks compose child tasks successfully

## 5) Risk Controls During Migration

- prefer `--plan` and `--dry-run` modes first (`test --plan`, `init --dry-run`, `migrate` preview)
- make one migration class at a time (manifest shape, then task routing, then CI JSON)
- keep lock recovery documented (`effigy unlock ...`) for interrupted dev flows
- use `effigy doctor --verbose` after each migration chunk

## 6) Quick Selector

- If you have no manifest: choose Path A.
- If you rely on legacy forwarding: choose Path B.
- If CI needs machine contracts: choose Path C.
- If teams split into subdomains: choose Path D.

## Related Guides

- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`027-copy-paste-snippets.md`](./027-copy-paste-snippets.md)

## Next Step

Choose one migration path and then run the validation bundle from [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md) before merging.
