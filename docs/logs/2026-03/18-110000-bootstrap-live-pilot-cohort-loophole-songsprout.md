# Bootstrap Live Pilot Cohort: Loophole and Songsprout

Status: complete
Created: 2026-03-18
Roadmap: g02.001
Batch: bootstrap-live-pilot-cohort-loophole-songsprout

## Summary

Validated `effigy bootstrap` against two real multi-repo workspaces with
different shapes:

- `loophole`: root orchestrator plus many app/runtime child repos
- `songsprout`: root orchestrator plus sibling `file:` dependency ordering and
  a separate docs-authority repo

The command surface and reporting held up, but the pilots exposed two concrete
adoption lessons:

- bootstrap contracts are currently a dev-build-only surface; released
  `effigy v0.2.9` still rejects `[bootstrap]`
- root-owned setup ordering matters when child repos install local
  `file:../...` dependencies

## Changes

### Loophole

- added a root `[bootstrap]` contract that clones `aura`, `chorus`, `composer`,
  `echo`, `pulse`, `pilot-repo-d`, and `spark`
- added root `bootstrap:deps` setup ownership so JS dependency bring-up is
  declared once at the workspace root
- set root `start = "remote:stack"` so long-running bring-up stays explicit
  behind `--start`
- updated root `README.md` and `AGENTS.md` to teach the bootstrap path
- removed redundant same-repo `--repo .` usage from `composer/package.json`

### Songsprout

- added a root `[bootstrap]` contract that clones `nursery`, `greenhouse`,
  `bloom`, `petal`, `stem`, `underlay`, and `trellis`
- added root `bootstrap:deps` setup ownership and refined the install order so
  `underlay` and `stem` exist before app shells that depend on them through
  `file:../...`
- set root `start = "dev"` so the workspace stack remains opt-in via
  `--start`
- updated root `README.md` and `AGENTS.md` to teach the bootstrap path
- removed redundant same-repo `--repo .` usage from `nursery`, `stem`,
  `petal`, `bloom`, and `greenhouse` package scripts

## Pilot Results

### Loophole

Live proof succeeded with the current Effigy dev build:

- root checkout cloned successfully
- all declared children cloned on `main`
- root `bootstrap:deps` ran successfully
- `aura` and `composer` dependency installs completed
- `remote:stack` remained configured but did not launch because `--start` was
  not supplied

The first attempt exposed a real manifest ergonomics issue:

- `bootstrap:deps` originally chained `cd aura && bun install` and
  `cd composer && bun install` without explicit subshell boundaries
- the second step inherited the first step's cwd and failed
- fixing the task to use explicit subshells made the contract stable

### Songsprout

The first live proof failed in a useful way:

- app-shell installs in `greenhouse` and `bloom` referenced sibling local
  packages through `file:../underlay` and `file:../stem`
- the initial bootstrap contract did not clone `underlay`
- the initial setup order installed app shells before the local dependency
  repos existed

After adding `underlay` as a child repo and reordering `bootstrap:deps`, the
live proof succeeded:

- root checkout cloned successfully
- all declared children cloned on `main`
- dependency installs completed in order
- `dev` remained configured but did not launch because `--start` was not
  supplied

## Validation

Validated with a mix of product and live-repo proof:

- `target/debug/effigy --json bootstrap <temp-root.git> --path <temp>/loophole`
  using current Loophole root files as the temporary source repo
- `target/debug/effigy --json bootstrap <temp-root.git> --path <temp>/songsprout`
  using current Songsprout root files as the temporary source repo
- `target/debug/effigy tasks --repo ~/Dev/projects/loophole`
- `target/debug/effigy tasks --repo ~/Dev/projects/songsprout`

## Decision

`g02.001` no longer needs more synthetic fixture work before the next
roadmap decision. The feature is now proven on:

- a root orchestrator with many child runtimes (`loophole`)
- a root orchestrator with sibling local package dependency ordering and a
  separate docs authority (`songsprout`)

The remaining product question is release readiness, not basic viability.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Movement: baseline `bootstrap runtime and reporting worked on synthetic
  fixtures only` -> current `bootstrap is proven on two real multi-repo
  workspaces, with concrete lessons about setup ordering and release-surface
  boundaries`
- Remaining gap: ship `[bootstrap]` in a released Effigy version, then decide
  whether one more pilot is needed before calling `g02.001` release-ready

## Next Task

Fold these pilot results into `g02.001`, mark Wave 1 as live-pilot validated,
and make the next decision explicitly: either run one final pilot on a third
workspace shape or start preparing bootstrap for a release once the released
binary surface can parse `[bootstrap]`.
