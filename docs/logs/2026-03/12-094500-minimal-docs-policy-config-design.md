# Minimal Docs Policy Config Design

Date: 2026-03-12
Owner: Platform

## Summary

Effigy needs a middle ground between two bad outcomes:

- hardcoded Effigy-repo docs doctrine inside built-ins
- a large bespoke config model that every adopting repo must complete before
  docs commands become useful

The recommended direction is a small optional `[docs_policy]` section in
`effigy.toml`. Generic built-in engines should continue to work with sensible
defaults and no extra config. Repos that want stricter policy can opt into a
small amount of declarative configuration.

## Vision Target Delta

- Primary tags touched: `CONTRACT`, `MAINT`, `OPERATE`
- Moved from `high-level warning not to hardcode vision policy` to `a concrete
  minimal config shape aligned with existing Effigy manifest patterns`
- Remains open: implementation of `[docs_policy]` parsing and the first
  config-backed engine migration

## Design Goals

- zero-config remains useful for common repos
- config stays optional and small
- built-ins provide generic engines, not repo doctrine
- repo tasks remain the place where a project bundles policy into its own QA
  flow
- no new standalone config file unless the manifest becomes clearly too crowded

## Recommended Config Shape

Use an optional manifest section:

```toml
[docs_policy]
roots = ["docs", "README.md"]
exclude = ["docs/logs/**"]

[docs_policy.indexes.logs]
file = "docs/logs/README.md"
section = "## Recent Validation Logs"
roots = ["docs/logs"]
pattern = "*.md"

[docs_policy.indexes.vision]
file = "docs/vision/README.md"
section = "## Vision Artifacts"
roots = ["docs/vision"]
pattern = "*.md"
exclude = ["docs/vision/history/**"]

[docs_policy.headings.files]
"docs/roadmaps/g01/001-effigy-foundation.md" = [
  "## Vision Alignment",
  "## Primary Tags",
  "## Target Envelope",
  "## Vision Target Delta",
]

[docs_policy.next_action]
enabled = true
index = "vision"
heading = "## Next Task"
allowlist_file = "docs/scripts/fixtures/vision-next-task/actionable-verbs.txt"

[docs_policy.history]
cutoff_date = "2026-03-06"
logs_require_delta = true
```

This is intentionally compact:

- `roots` / `exclude` let built-ins scan conventional docs locations without
  forcing every command to take long path lists
- `indexes` describes reusable "index file points at artifact set" checks
- `headings.files` expresses exact-file governance rules when a repo wants them
- `next_action` captures the policy currently buried in
  `check-vision-next-task.sh`
- `history` captures forward-only reporting policy where needed

## Why `effigy.toml`

This matches existing Effigy configuration style:

- `[release]` and `[release.gates]` keep release policy close to the repo
- `[env_schema]` adds optional behavior without making the manifest mandatory
  for all projects
- task composition already lives in `effigy.toml`

Using `[docs_policy]` keeps docs governance local to the repo contract and
avoids inventing another top-level config file too early.

## Defaults vs Policy

### Built-in defaults that should remain zero-config

- `effigy docs check-links`
  - defaults to `README.md` plus recursive `docs/`
- `effigy docs check-json-examples`
  - defaults to current repo-local example target when explicitly wired by task
- `effigy docs check-index`
  - defaults to the current logs index contract unless a named index is passed
- `effigy docs check-workflow-paths`
  - defaults to markdown under `docs/` and `README.md`, excluding `docs/logs/`

### Policy that should move behind config

- named indexed artifact sets such as "vision"
- required headings for exact roadmap/guide files
- next-action heading names and allowlist files
- history-specific exclusions and forward-only cutoffs
- strategy-specific placement rules for active vs archived docs

## Migration Plan Enabled By This Config

Phase 1:

- add parser support for optional `[docs_policy]`
- keep all existing built-ins working without it
- expose lookup helpers for named indexes and policy bundles

Phase 2:

- extract a generic config-backed index checker
- migrate `docs/scripts/check-vision-index.sh` to a built-in that uses the
  named `vision` index config

Phase 3:

- decide whether `next_action` policy becomes a built-in engine behind config
  or remains repo-local task/script logic
- only move it if the config surface stays small and clearly reusable

## Non-Goals

- do not make every docs command require `[docs_policy]`
- do not create a giant schema with every possible markdown lint rule
- do not fold Effigy's full governance model into generic defaults
- do not remove repo-local tasks as the main place where projects define their
  QA bundles

## Recommended First Implementation Slice

The first config-backed migration should be the vision index check, not the
next-task policy:

- it is structurally closer to the existing built-in log-index work
- it has clearer reuse outside Effigy
- it avoids prematurely standardizing the actionable-verb policy as a generic
  product feature

After that, re-evaluate whether the remaining `docs/scripts/check-vision-*.sh`
surface is small enough to leave repo-local indefinitely.
