# Songsprout Trellis Authority-Only Pilot

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: songsprout-trellis-authority-only-pilot

## Summary

Applied the released `effigy v0.2.6` Northstar + Effigy consumer contract to
`songsprout/trellis` without changing the `songsprout` workspace root.

This pilot was intentionally narrower than the earlier workspace-container
cohort because the root `AGENTS.md` currently says not to modify root files
other than `AGENTS.md`. Instead of overriding that guardrail implicitly, this
batch treated `trellis` as the migration target and left the root repo contract
unchanged.

## Changes

- upgraded `trellis/effigy.toml` with:
  - declarative `[docs_policy.indexes.vision]`
  - declarative `[docs_policy.next_actions.vision]`
  - native `qa:docs`
  - native `qa:northstar`
- normalized `trellis/README.md` and `trellis/AGENTS.md` so they teach repo-root
  `effigy` usage without redundant `--repo .`
- added repo-owned vision next-task policy in
  `trellis/policy/vision-next-task-verbs.txt`
- tightened Trellis index docs and the primary vision doc so they now include
  explicit `## Next Task` sections and a valid `## Vision Artifacts` list:
  - `trellis/vision/README.md`
  - `trellis/vision/001-songsprout-platform-vision.md`
  - `trellis/roadmaps/README.md`
  - `trellis/roadmaps/g01/README.md`
  - `trellis/roadmaps/generation-index.md`
  - `trellis/logs/README.md`

## Validation

Validated directly in `songsprout/trellis` against released `effigy v0.2.6`:

- `effigy qa:northstar`
- `effigy qa:docs`

Both passed.

## Decision

This batch proves a new adoption mode explicitly:

- frozen or intentionally thin workspace root
- docs authority migrated independently
- native docs-policy and docs QA applied only where the repo contract allows

That is different from the earlier `example-site` and `underlay-reference`
cohort, where the workspace roots were also updated to expose root-level
`qa:docs` and `qa:northstar` orchestration.

The remaining choice for Songsprout is governance, not tooling:

- either keep the current root guardrail and let `trellis` remain the only
  native docs-policy surface
- or explicitly relax the root rule later and add root-level orchestration
  delegation into `trellis`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Movement: baseline `workspace-container adoption assumed root plus authority
  migration together` -> current `the contract now also supports authority-only
  migration when a workspace root is intentionally frozen`
- Remaining gap: `songsprout` root orchestration still reflects older Effigy
  defaults because the current repo guardrail intentionally prevented widening
  this batch`

## Next Task

Decide whether `songsprout` should stay authority-only at the root or whether
the root guardrail should be relaxed in a later batch so root `qa:docs` /
`qa:northstar` delegation can be added cleanly.
