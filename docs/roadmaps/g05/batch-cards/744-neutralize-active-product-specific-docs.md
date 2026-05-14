# 744 - Neutralize Active Product-Specific Docs

Roadmap: [`../023-active-docs-product-neutrality-cleanup.md`](../023-active-docs-product-neutrality-cleanup.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Align active docs and contracts with the reusable-core posture without
rewriting historical evidence.

## Scope

- archive, relabel, or de-anchor product-specific active contract references
- refresh active guides that still imply built-in Render/Railway behavior
- add the missing YAML helper documentation to the Rhai host-surface audit

## Acceptance

- active docs stop presenting product-specific bundles as core anchors
- provider docs describe external provider packages accurately
- historical references remain intact where they are supposed to

## Outcome

- updated `docs/contracts/README.md` so old Underlay and Decodelabs contracts
  are retained as historical/example evidence instead of active reusable-core
  anchors
- clarified active provider docs so Render and Railway are described as
  configured external deploy-provider packages rather than built-in core
  behavior
- added YAML helpers to the Rhai host-surface audit
- intentionally retained historical product-specific contracts and examples in
  place

## Stop Conditions

- stop if neutrality requires a new contract rather than a bounded docs/index
  refresh

## Validation

- `effigy docs check paths docs/contracts/README.md docs/guides/025-command-reference-matrix.md docs/guides/068-rhai-host-surface-audit.md docs/guides/074-deployment-guide.md`
- `git diff --check`
- `rg -n "Underlay|Decodelabs|underlay|decodelabs|Railway|Render|railway|render" docs/contracts docs/guides -g '!docs/logs/**' -g '!**/CHANGELOG*'`

## Next Task

Execute `745`.
