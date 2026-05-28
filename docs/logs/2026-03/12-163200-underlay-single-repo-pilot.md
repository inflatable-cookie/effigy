# Underlay Single-Repo Pilot

Status: complete
Created: 2026-03-12

## Summary

Applied the Northstar + Effigy consumer contract to `underlay` as the third
pilot and confirmed the simple native single-repo path works on a shared
foundation repo as well as on an app repo.

## What Changed

- removed redundant current-repo `--repo .` usage from active operator-facing
  surfaces
- made the TypeScript/Svelte task layer Bun-native instead of assuming direct
  `tsc`, `svelte-check`, or `vitest` binaries
- added native `qa:docs` and `qa:northstar` bundles in `effigy.toml`
- added a minimal docs-policy contract for the vision index and next-action
  rule
- normalized the vision and roadmap front doors so the docs contract can be
  enforced mechanically

## Pilot Outcome

This pilot matters because it proves the contract is not only for app repos.
The same operating model works for a shared foundation repo with a large docs
corpus and a mixed Rust/TypeScript surface.

The main remaining gap is maturity, not structure:

- `underlay` now has the operator/docs contract
- `underlay` does not yet have a changelog or release posture
- that means the repo is contract-aligned for daily use, but not yet at the
  full release-ready consumer-contract bar

## Validation

Passed:

- `PATH="$HOME/.local/bin:$PATH" effigy qa:docs` in `underlay`
- `PATH="$HOME/.local/bin:$PATH" effigy qa:northstar` in `underlay`
- `PATH="$HOME/.local/bin:$PATH" effigy qa` in `underlay`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Moved from `Northstar + Effigy contract proven mainly on app repos and one
  workspace-docs-authority split repo` to `same contract also proven on a
  shared single-repo foundation project with native docs validation`
- Remains open: add changelog and release posture where appropriate, decide
  whether shared foundation repos should adopt the same release baseline as app
  repos, and test the contract on a more orchestration-heavy consumer such as
  `example-app`
