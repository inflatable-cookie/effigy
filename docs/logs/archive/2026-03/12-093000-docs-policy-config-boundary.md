# Docs Policy Config Boundary

Date: 2026-03-12
Owner: Platform

## Summary

The remaining shell validation surface under `docs/scripts/` is not generic
docs QA in disguise. It is mostly Effigy-specific docs policy:

- `check-vision-index.sh` enforces the structure and inventory of
  `docs/vision/README.md`
- `check-vision-next-task.sh` enforces the presence and wording shape of
  `## Next Task` sections across vision artifacts
- `check-vision-next-task-regression.sh` protects that policy with fixtures
- `check-vision-metadata.sh` aggregates those checks plus a small amount of
  cross-doc heading/cutoff policy

That distinction matters. Generic engines belong in Effigy built-ins. Effigy's
current vision governance does not.

## Vision Target Delta

- Primary tags touched: `CONTRACT`, `MAINT`, `OPERATE`
- Moved from `implicit caution about docs-policy hardcoding` to `explicit
  migration boundary and config-first follow-on direction`
- Remains open: a minimal docs-policy config surface and a decision on which
  remaining vision checks should become config-backed engines versus stay
  repo-local

## What Is Generic Enough For Built-ins

These capabilities are reasonable built-in engines because another repo could
reuse them without patching Effigy:

- scan markdown files under conventional docs roots
- validate relative file links
- validate JSON fenced examples
- validate file-index consistency
- scan docs for workflow-path references and flag stale `.github-bak` or
  missing `.github/workflows/*.yml` targets
- insert a log entry into an indexed report file

These commands can ship with useful defaults such as `docs/` and `README.md`,
as long as those defaults are conventions rather than mandatory doctrine.

## What Should Not Be Hardcoded

The following are repo-policy concerns and should not become built-in defaults
without a config boundary:

- a required `docs/vision/README.md` inventory model
- exact section names such as `## Vision Artifacts`
- exact section names such as `## Next Task`
- Effigy's actionable-verb allowlist
- forward-only policy cutoffs like `2026-03-06`
- exact required roadmap/guide headings for Effigy's own governance model
- strategy-specific file placement rules such as "closeout artifacts must move
  from `docs/vision/` to `docs/vision/history/`"

Another project may want similar checks, but those rules need to come from
config or repo-local task composition, not from Rust constants in Effigy.

## Recommended Middle Ground

Adopt a three-layer model:

1. Generic built-in engines

- `effigy docs check-links`
- `effigy docs check-index`
- `effigy docs check-json-examples`
- `effigy docs check-workflow-paths`

2. Small optional docs-policy config

The config should stay intentionally small. It should describe policy, not
force a full schema authoring exercise before the commands are useful.

Examples of acceptable config inputs:

- docs roots to scan
- paths to exclude
- index file path and indexed section heading
- required headings for a declared set of files
- optional "next action" section heading name
- optional allowlist file path for lead verbs
- optional historical directory rules

3. Repo-local task composition

Effigy itself can define opinionated tasks such as:

- `qa:docs:vision`
- `qa:docs:policy`

Other repos can ignore those tasks, use only the generic engines, or bind them
to different policy files.

## Migration Rule Going Forward

Before migrating any more `docs/scripts/check-vision-*.sh` logic into built-ins:

1. define the minimal docs-policy config surface
2. move only reusable engines behind that config
3. keep Effigy-specific doctrine in repo config, fixtures, and task wiring

Until that exists, the remaining vision-policy scripts should stay repo-local.

## Candidate Follow-on Split

The safest next tranche is:

- extract a generic "indexed artifact set" engine that can be pointed at
  `docs/vision/README.md` by config
- leave `next-task` policy and allowlist semantics repo-local until the config
  shape is designed
- keep `check-vision-metadata.sh` as the policy bundle wrapper for now

This keeps Effigy broadly reusable without requiring every adopting repo to
author a large docs-policy manifest on day one.
