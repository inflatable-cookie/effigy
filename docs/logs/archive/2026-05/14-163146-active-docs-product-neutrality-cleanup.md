# 2026-05-14 16:31:46 - Active Docs Product Neutrality Cleanup

Roadmap: [`g05.020`](../roadmaps/g05/020-reusable-core-hardening-suite.md)  
Batch card: [`744`](../roadmaps/g05/batch-cards/744-neutralize-active-product-specific-docs.md)  
Strict lane: [`083`](../specs/083-reusable-core-hardening-strict-lane.md)

## What Changed

- changed `docs/contracts/README.md` so product-specific contracts are framed
  as retained historical/example evidence rather than active reusable-core
  anchors
- updated deployment-facing guides to describe Render and Railway through
  configured external deploy-provider packages
- updated the Rhai host-surface audit to include YAML helpers

## Intentionally Retained

- `docs/contracts/003-underlay-deployment-derivation.md`
- `docs/contracts/004-underlay-reference-deploy-model-example.md`
- `docs/contracts/007-render-export-contract.md`
- `docs/contracts/008-railway-export-contract.md`
- `docs/contracts/010-decodelabs-production-strategy.md`

These remain as historical or example evidence. They are no longer presented by
the contracts index as active reusable-core anchors.

## Validation

- `effigy docs check paths docs/contracts/README.md docs/guides/025-command-reference-matrix.md docs/guides/068-rhai-host-surface-audit.md docs/guides/074-deployment-guide.md`
- `git diff --check`
- `rg -n "Underlay|Decodelabs|underlay|decodelabs|Railway|Render|railway|render" docs/contracts docs/guides -g '!docs/logs/**' -g '!**/CHANGELOG*'`
