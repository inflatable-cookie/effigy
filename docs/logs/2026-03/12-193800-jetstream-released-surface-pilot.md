# Jetstream Released-Surface Pilot

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: jetstream-released-surface-pilot

## Summary

Applied the Northstar + Effigy consumer contract to `jetstream` on released
`effigy v0.2.6` and confirmed that the released docs-policy surface works even
when the repo already has a large research-heavy docs tree and a custom legacy
docs contract script.

This pilot matters because it was not a clean-room adoption. The native
`qa:docs` and `qa:northstar` surfaces were installed successfully, then the
docs lane exposed real backlog debt in concept, prototype, and research links.
Once that debt was repaired, the full native docs lane passed without backing
away from the released Effigy surface.

## Changes

- normalized `jetstream/AGENTS.md`, `jetstream/README.md`, and
  `jetstream/effigy.toml` so the repo teaches root-level `effigy` usage and
  routes docs validation through native `qa:docs` / `qa:northstar`
- added declarative `[docs_policy.indexes.vision]` and
  `[docs_policy.next_actions.vision]` config in `jetstream/effigy.toml`
- tightened `jetstream/docs/README.md`, `jetstream/docs/vision/README.md`,
  `jetstream/docs/roadmaps/README.md`, and `jetstream/docs/logs/README.md`
  around explicit `## Next Task` headings and valid index behavior
- added `jetstream/docs/policy/vision-next-task-verbs.txt` so the repo owns
  its vision next-task vocabulary
- repaired high-volume broken-link debt in concept, prototype, and research
  docs by turning planned-but-missing concept pages into plain text references,
  correcting relative research paths, and aligning stale research links to the
  actual file inventory
- kept `scripts/check-doc-contracts.sh` as a compatibility subtask inside the
  new native docs lane so Jetstream retains its repo-specific concept/report
  policy checks without replacing the released Effigy surface

## Validation

Validated directly in `jetstream` against released `effigy v0.2.6`:

- `effigy tasks`
- `effigy qa:northstar`
- `effigy qa:docs`

`effigy qa:docs` now passes end to end, including:

- `effigy docs check-links`
- `effigy docs check-forbidden`
- `effigy docs check-headings`
- `effigy docs check-index --policy-index vision`
- `effigy docs check-next-action --policy vision`
- `bash scripts/check-doc-contracts.sh`

## Decision

The released `0.2.6` surface is now proven on a harder class of consumer repo:

- `signal` proved native docs-policy adoption on a straightforward consumer
  repo
- `convergence` proved coexistence with a broader existing validation lane
- `jetstream` proves the same released docs-policy surface can absorb real docs
  debt and coexist with a repo-owned legacy docs contract script

The remaining cross-cutting product gap is unchanged: `effigy release
verify-install` still needs SSH-remote normalization so post-release
verification is as portable as consumer docs adoption.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`, `RELEASE`
- Movement: baseline `released 0.2.6 docs-policy path proven mainly on cleaner
  consumer repos` -> current `released 0.2.6 docs-policy and repo-owned docs
  QA proven on a research-heavy repo with legacy custom docs-policy checks and
  real backlog cleanup`
- Remaining gap: `release verify-install` still fails on SSH-style remotes, so
  release closeout portability still lags behind consumer docs portability`

## Next Task

Migrate another repo with either a split authority model or a similarly
heavyweight docs tree, then fix the `release verify-install` SSH-remote
handling so post-release validation matches the now-proven consumer adoption
surface.
