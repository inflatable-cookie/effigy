# Signal Released-Surface Pilot

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: signal-released-surface-pilot

## Summary

Applied the Northstar + Effigy consumer contract to `signal` after the `0.2.6`
release and confirmed that the released binary now supports the native docs
surface needed for consumer adoption:

- `effigy docs check-index --policy-index ...`
- `effigy docs check-next-action --policy ...`
- repo-owned `qa:docs`
- repo-owned `qa:northstar`

This is the first non-Effigy consumer repo in the sweep that proves those
surfaces on the released binary rather than on a local dev build.

## Changes

- normalized `signal/AGENTS.md` and `signal/README.md` so they teach repo-root
  `effigy tasks`, `effigy doctor`, `effigy health`, `effigy validate`, and
  `effigy qa:docs` instead of redundant current-directory `--repo .` usage
- added native `qa:docs` and `qa:northstar` task composition to
  `signal/effigy.toml`
- added declarative `[docs_policy.indexes.vision]` and
  `[docs_policy.next_actions.vision]` configuration in `signal/effigy.toml`
- added `signal/docs/policy/vision-next-task-verbs.txt` so the vision next-task
  contract is repo-owned instead of hardcoded in generic tooling
- tightened `signal/docs/vision/README.md` into a valid markdown index with a
  canonical `## Next Task` heading and explicit file links
- recorded the adoption in `signal/CHANGELOG.md`

## Validation

Validated directly in `signal` against released `effigy v0.2.6`:

- `effigy tasks`
- `effigy qa:northstar`
- `effigy qa:docs`

All passed without any dev-binary path overrides or repo-local fallback
scripts.

## Decision

The released `0.2.6` binary is now sufficient for the docs-validation side of
consumer adoption on a real single-repo app/foundation codebase that already
has a Northstar docs spine.

The remaining released-surface gap is narrower now:

- consumer docs-policy adoption is proven on a released binary
- release/install verification still needs remote-URL normalization work, as
  `effigy release verify-install --tag v0.2.6` currently fails when a repo is
  configured only with `git@github.com:...` SSH remotes

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`, `RELEASE`
- Movement: baseline `consumer docs-policy adoption proven mainly on local dev
  binaries` -> current `released effigy v0.2.6 proves native docs-policy and
  repo-owned docs QA on a real non-Effigy consumer repo`
- Remaining gap: `release verify-install` still needs SSH-remote normalization
  so post-release verification is as portable as the docs surface`

## Next Task

Use the released `0.2.6` surface to migrate the next repo with an existing
Northstar spine, then fix the `release verify-install` SSH-remote bug so the
post-release path is as reliable as consumer docs adoption.
