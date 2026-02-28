# Legacy And Active Repo Smoke Sweep

Date: 2026-02-26
Owner: platform
Related roadmap: 002-deferral-fallback-system

## Scope
- Validate direct source-run usage (`cargo run --manifest-path ... --bin effigy -- ...`) against:
- `/Users/betterthanclay/Dev/legacy/sites/r7-playground` (legacy PHP-effigy project)
- `/Users/betterthanclay/Dev/projects/acowtancy` (active effigy-catalog workspace)
- Confirm expected behavior for:
- built-in task (`repo-pulse`)
- catalog listing (`tasks`)
- unresolved request (`version`) with deferral rules

## Changes
- No code changes in this sweep.
- Captured runtime compatibility evidence after:
- implicit root deferral fallback (`<legacy defer process> {request} {args}`) for unresolved task requests
- generalized `composer.json` root marker support

## Validation
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- repo-pulse --repo /Users/betterthanclay/Dev/legacy/sites/r7-playground`
  - result: exit 0, pulse report rendered, root markers include `package.json, composer.json, .git`.
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- tasks --repo /Users/betterthanclay/Dev/legacy/sites/r7-playground`
  - result: exit 1, expected error: no `effigy.toml` catalogs found.
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- version --repo /Users/betterthanclay/Dev/legacy/sites/r7-playground`
  - result: exit 0, implicit fallback executed legacy global effigy (`Effigy : v0.10.11`).
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- repo-pulse --repo /Users/betterthanclay/Dev/projects/acowtancy`
  - result: exit 0, pulse report rendered, root markers include `package.json, .git`.
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- tasks --repo /Users/betterthanclay/Dev/projects/acowtancy`
  - result: exit 0, catalogs and tasks rendered (acowtancy/cream/dairy/farmyard).
- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- version --repo /Users/betterthanclay/Dev/projects/acowtancy`
  - result: exit 1, expected unresolved-task error, no deferral engaged.

## Risks / Follow-ups
- `r7-playground` currently depends on unresolved-task deferral because no `effigy.toml` exists there.
- `tasks` command intentionally fails when no catalogs are present; if desired, we can make this a softer empty-state for legacy-only repos.

## Next
- Keep unresolved-task deferral enabled for legacy projects during migration.
- Add explicit `effigy.toml` to legacy repos as they are actively migrated, then remove fallback reliance per-repo.
