# g05.023 - Active Docs Product Neutrality Cleanup

Status: Planned
Depends on: `g05.021`

## Goal

Make active Effigy docs and contracts describe a reusable core instead of
presenting Underlay, Decodelabs, Acowtancy, Render, or Railway as built-in core
ownership.

Historical docs can keep historical names. Active docs should be neutral and
accurate.

## Evidence

- `docs/contracts/README.md` still lists active Underlay and Decodelabs
  contracts
- `docs/contracts/003-underlay-deployment-derivation.md`,
  `004-underlay-reference-deploy-model-example.md`, and
  `010-decodelabs-production-strategy.md` remain active contract entries
- provider docs now correctly say Render and Railway live in deploy-provider
  packages, but active contract anchors still mix product-specific and core
  concerns
- the 2026-05-14 audit found `docs/guides/068-rhai-host-surface-audit.md`
  missing the YAML helpers now exposed for provider scripts

## Scope

- update active contract index language so product-specific contracts are
  archived, renamed as examples, or clearly marked historical
- keep provider-neutral deployment model docs as the active core anchor
- keep deploy-provider package docs accurate for Render/Railway as external
  packages
- update Rhai host-surface docs to include YAML helpers
- check guides that mention Render/Railway and ensure they say “configured
  provider package”, not built-in adapter
- leave changelogs, logs, archived specs, and historical roadmaps alone

## Out Of Scope

- no source code changes unless docs validation requires path updates
- no removal of historical names from archived materials
- no edits under `external/`
- no new marketing or tutorial rewrite

## Guardrails For A Cheaper Model

- before deleting or moving a contract, check whether another active doc links
  to it
- if a product-specific contract still has useful generic content, extract the
  generic rule into a neutral contract and archive the product-specific file
- keep examples generic unless the doc is explicitly about an external provider
  package
- use “provider package” for Render/Railway capability; avoid “built-in”
  wording
- do not scrub commit history, changelog, or planning evidence

## Suggested Implementation Steps

1. Search active docs only:
   `rg -n "underlay|decodelabs|Acowtancy|Render|Railway" docs/contracts docs/guides`.
2. Classify each hit as active guidance, external package example, or historical
   reference.
3. Update active contract index and drift-trigger tables.
4. Refresh `docs/guides/068-rhai-host-surface-audit.md` for YAML.
5. Run docs/path/reference checks used by this repo.
6. Record any intentionally retained product names.

## Acceptance Criteria

- active contracts do not present product-specific bundles as core Effigy
  anchors
- Render/Railway docs accurately describe external provider packages
- Rhai host-surface docs list YAML read/write helpers
- historical references remain intact where appropriate
- docs validation passes

## Validation

Minimum focused validation:

```bash
rg -n "underlay|decodelabs|Acowtancy|Render|Railway" docs/contracts docs/guides
effigy docs check paths
effigy docs check contains
```

Use the repo’s current docs QA task if available.

## Next Task

After docs neutrality is handled, move to `g05.024` for the larger state-domain
thin-shell follow-through.
