# Compli-me Workspace + Docs Authority Pilot

Status: complete
Created: 2026-03-12

## Summary

Applied the Northstar + Effigy consumer contract to `pilot-repo-b` as the second
pilot and confirmed it is not the same adoption shape as `pilot-repo-a`.

The workspace root remains a thin orchestration repo. The nested `docs/` repo
is the real documentation authority and now carries the native current-Effigy
docs contract.

## What Changed

- removed redundant current-repo `--repo .` usage from the workspace root
  `AGENTS.md`, `README.md`, and `package.json`
- changed the workspace-root `validate` path to call `docs/qa` so the root
  Effigy surface pulls in the docs-authority contract instead of only the old
  rollout checks
- added native `qa:docs` and `qa:northstar` task bundles plus `[docs_policy]`
  for the `docs/` repo
- repaired the docs front-door link graph and normalized the vision/roadmap
  indexes so the native docs checks can enforce them
- rewrote the docs-authority guidance to lead with `effigy tasks`,
  `effigy health`, `effigy qa:docs`, `effigy qa:northstar`, and `effigy qa`

## Pilot Outcome

The reusable lesson is a third adoption shape:

- single consumer repo: full contract lives at the repo root
- compatibility consumer repo: full contract lives at the repo root but uses
  repo-owned doctrine scripts
- workspace container + docs authority repo: keep the workspace root thin and
  apply the real Northstar contract inside the nested docs-authority repo

This means the skill and contract docs must explicitly support a split model
instead of assuming every consuming project has one top-level docs/release
surface.

## Validation

Passed:

- `PATH="$HOME/.local/bin:$PATH" effigy qa:docs` in `pilot-repo-b/docs`
- `PATH="$HOME/.local/bin:$PATH" effigy qa:northstar` in `pilot-repo-b/docs`
- `PATH="$HOME/.local/bin:$PATH" effigy docs/qa` from `pilot-repo-b` root

Blocked outside this batch:

- `PATH="$HOME/.local/bin:$PATH" effigy qa` from `pilot-repo-b` root still fails
  in existing frontend/tooling validation because `tsc` is not available in the
  current environment

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Moved from `Northstar + Effigy adoption modeled mostly as a single-repo
  contract` to `consumer adoption contract proven in both single-repo and
  workspace-root-plus-docs-authority forms`
- Remains open: encode the workspace-container mode in the portable skill
  bundle, decide whether changelog/release baselines belong in docs-authority
  repos or only in releasable code repos, and prove the refined mode on a third
  consumer repo
