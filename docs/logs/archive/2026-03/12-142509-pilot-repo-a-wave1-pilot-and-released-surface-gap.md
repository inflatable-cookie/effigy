# Monkey Wave 1 Pilot And Released Surface Gap

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: pilot-repo-a-wave1-pilot

## Summary

- Applied the Wave 1 Northstar + Effigy consumer contract to `pilot-repo-a` as the
  first real pilot.
- Rewrote `pilot-repo-a`'s agent and front-door docs around the Effigy-first loop:
  `effigy tasks`, `effigy doctor`, `effigy test --plan`, and `effigy qa`.
- Added `CHANGELOG.md` plus repo-owned `qa:docs` and `qa:northstar` bundles so
  the contract is inspectable through Effigy tasks.
- Confirmed that the contract works on the released Effigy surface only when
  consumer repos use repo-owned scripts for docs and release-readiness checks.

## Changes

- `pilot-repo-a` now uses built-in `effigy test` instead of a local `tasks.test`
  override.
- `pilot-repo-a` now has:
  - Effigy-first `AGENTS.md`
  - README and docs front door aligned to the same operator loop
  - `CHANGELOG.md` with an `Unreleased` baseline
  - `qa:docs` surfaced through `docs/validate` plus a consumer-contract script
  - `qa:northstar` surfaced through a repo-owned Northstar contract script
- The pilot exposed a released-surface boundary:
  - released Effigy `v0.2.4` does not accept consumer-side `[docs_policy]`
  - released Effigy `v0.2.4` does not accept consumer-side `[release]`
  - released Effigy `v0.2.4` does not expose `effigy docs ...` subcommands in
    consumer repos

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: baseline `consumer adoption kit still theoretical outside the
  Effigy repo` -> current `manual consumer pilot proven in pilot-repo-a on the
  released Effigy surface`
- Remaining gap: `consumer repos still need repo-owned scripts for docs and
  release-readiness validation because the released product surface has not yet
  caught up with the internal design`

## Validation Performed

- command: `effigy tasks`
  - result: passed in `~/Dev/projects/pilot-repo-a`; showed the new consumer contract
    tasks
- command: `effigy test --plan`
  - result: passed in `~/Dev/projects/pilot-repo-a`; confirmed built-in test routing
    to `cargo nextest run`
- command: `effigy qa:docs`
  - result: passed in `~/Dev/projects/pilot-repo-a`
- command: `effigy qa:northstar`
  - result: passed in `~/Dev/projects/pilot-repo-a`
- command: `effigy qa`
  - result: passed in `~/Dev/projects/pilot-repo-a`; 84 tests passed, 3 skipped

## Risks

- If Effigy doctrine keeps assuming unreleased docs/release primitives in
  consumer repos, the adoption kit will teach broken manifests.
- If the skill is written before this released-surface boundary is encoded, it
  will overfit to the Effigy repo and fail in consuming apps.

## Next Task

Turn the pilot finding into product and skill boundaries: define the temporary
consumer contract as `Effigy-first tasks plus repo-owned validation scripts`,
then scope the native consumer docs/release surface needed to replace those
scripts in a later Effigy milestone.
