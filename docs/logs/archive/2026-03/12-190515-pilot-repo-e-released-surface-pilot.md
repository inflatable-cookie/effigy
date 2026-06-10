# Convergence Released-Surface Pilot

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: pilot-repo-e-released-surface-pilot

## Summary

Applied the Northstar + Effigy consumer contract to `pilot-repo-e` on released
`effigy v0.2.6` and confirmed that the released binary now supports the native
docs-validation path on another real non-Effigy repo with an established
Northstar docs spine.

Unlike the earlier `pilot-repo-d` batch, this pilot also completed the repo-owned
runtime validation path after the docs contract was installed, showing that the
new task graph coexists cleanly with an already substantial existing test
surface.

## Changes

- normalized `convergence/AGENTS.md` and `convergence/README.md` so they teach
  repo-root `effigy` usage without redundant current-directory `--repo .`
  defaults
- added native `qa:docs` and `qa:northstar` task composition to
  `convergence/effigy.toml`
- added declarative `[docs_policy.indexes.vision]` and
  `[docs_policy.next_actions.vision]` config in `convergence/effigy.toml`
- added `convergence/docs/policy/vision-next-task-verbs.txt` so the vision
  next-task contract is repo-owned
- tightened the root docs indexes (`docs/README.md`, `docs/vision/README.md`,
  `docs/roadmaps/README.md`, `docs/logs/README.md`) around explicit
  `## Next Task` headings and valid markdown index entries
- repaired stale broken links in
  `convergence/docs/research/specimen-dossiers/README.md` so docs QA reflects
  the real dossier inventory rather than pending future placeholders

## Validation

Validated directly in `pilot-repo-e` against released `effigy v0.2.6`:

- `effigy tasks`
- `effigy qa:northstar`
- `effigy qa:docs`
- `effigy validate`

`effigy validate` completed successfully after one transient stale workspace
lock cleared; the underlying run finished with `66` tests passed and `0`
skipped.

## Decision

The released `0.2.6` surface is now proven on more than one non-Effigy
consumer repo:

- `pilot-repo-d` proved the docs-policy and docs-QA path on a repo with an active
  changelog and mixed runtime surface
- `pilot-repo-e` proves the same docs-policy path coexists with a deeper
  existing validation lane and does not require repo-specific shell fallback

The main remaining product gap is still outside the consumer docs surface:
`effigy release verify-install` needs SSH-remote normalization so post-release
verification is as portable as consumer adoption.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`, `RELEASE`
- Movement: baseline `released 0.2.6 docs-policy path proven on one
  non-Effigy repo` -> current `released 0.2.6 docs-policy and repo-owned docs
  QA proven on multiple real consumer repos, including one with a substantial
  existing validation lane`
- Remaining gap: `release verify-install` still fails on SSH-style remotes, so
  release closeout portability still lags behind consumer adoption portability`

## Next Task

Migrate the next repo that already has `docs/vision` and `docs/roadmaps` on
released `0.2.6`, then fix the `release verify-install` SSH-remote handling so
post-release validation catches up with the now-proven consumer docs surface.
