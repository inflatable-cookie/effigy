# Workspace Docs-Authority Cohort: Contact Patch + Underlay Reference

Status: complete
Created: 2026-03-12
Roadmap: g01.029
Batch: workspace-docs-authority-cohort-example-site-underlay-reference

## Summary

Applied the released `effigy v0.2.6` Northstar + Effigy consumer contract to
two more clean workspace-container repos:

- `example-site`
- `underlay-reference`

Both repos already had thin workspace roots plus dedicated docs-authority
folders (`cp-docs/` and `acme-docs/`). This batch standardized that shape on
the released consumer surface:

- workspace roots now teach root-level `effigy` usage without redundant
  `--repo .`
- workspace roots expose `qa:docs` and `qa:northstar` as orchestration surfaces
- docs-authority catalogs now own native `qa:docs`, `qa:northstar`, and
  declarative vision next-action policy

The batch also surfaced one important runtime nuance: child docs-authority
catalogs inside a larger workspace still need internal `--repo .` wiring in
their own manifest tasks so built-in docs-policy checks resolve against the
child authority root instead of the workspace root. That remains an internal
implementation detail, not something taught to agents or maintainers.

## Changes

- normalized root teaching surfaces in:
  - `example-site/AGENTS.md`
  - `example-site/README.md`
  - `example-site/package.json`
  - `example-site/effigy.toml`
  - `underlay-reference/AGENTS.md`
  - `underlay-reference/README.md`
  - `underlay-reference/package.json`
  - `underlay-reference/effigy.toml`
- upgraded `cp-docs/` and `acme-docs/` with:
  - declarative `[docs_policy.indexes.vision]`
  - declarative `[docs_policy.next_actions.vision]`
  - native `qa:docs`
  - native `qa:northstar`
  - root-to-authority execution guidance via explicit catalog selectors
- added repo-owned vision next-task allowlists in:
  - `example-site/cp-docs/policy/vision-next-task-verbs.txt`
  - `underlay-reference/acme-docs/policy/vision-next-task-verbs.txt`
- normalized docs authority indexes and primary vision docs to include explicit
  `## Next Task` sections and valid `## Vision Artifacts` inventories

## Validation

Validated directly on released `effigy v0.2.6`:

- in `example-site`:
  - `effigy cp-docs/qa:docs`
  - `effigy qa:docs`
- in `underlay-reference`:
  - `effigy acme-docs/qa:docs`
  - `effigy qa:docs`

All passed.

## Decision

The released `0.2.6` surface is now proven across another important adoption
shape:

- thin workspace root
- dedicated docs-authority catalog inside the same repo
- root-level docs orchestration delegated into that authority

This batch also clarifies a product boundary:

- agent-facing guidance should still omit redundant `--repo .`
- child docs-authority catalogs in the same workspace currently need internal
  local-root task wiring for docs-policy built-ins to bind to the correct
  authority root

`songsprout` was not included in this same cohort even though it is broadly
similar, because its current root guardrail explicitly says not to modify root
files beyond `AGENTS.md`. That needs a deliberate override or a trellis-only
batch rather than being folded into this same no-surprises sweep.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`, `MAINT`
- Movement: baseline `released 0.2.6 had proven single repos and a few
  workspaces, but not another clean thin-root plus docs-authority cohort` ->
  current `released 0.2.6 now proves that workspace roots and nested
  docs-authority catalogs can standardize on native docs QA while keeping
  agent-facing commands free of redundant current-dir repo flags`
- Remaining gap: `songsprout` still needs a deliberate handling decision, and
  child-catalog docs-policy still depends on internal local-root task wiring`

## Next Task

Take the same workspace-container contract to `songsprout`, but do it as a
deliberate boundary decision: either relax the current root-edit guardrail and
migrate the full root plus `trellis`, or keep the root intentionally thin and
move only `trellis` onto the native docs-policy surface.
