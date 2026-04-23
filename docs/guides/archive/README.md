# Effigy Guides Archive

This directory holds guides that were useful at one point but no longer belong
in the primary guide set. Archival policy lives in
[`../040-docs-archive-and-deprecation-policy.md`](../040-docs-archive-and-deprecation-policy.md).

Guides are kept in-tree so old logs, PRs, and external docs that linked here
don't dead-end. Do not treat them as active guidance.

## Archived Guides

| Guide | Archived Because | Active Replacement |
|-------|------------------|--------------------|
| [`028-docs-flow-map.md`](./028-docs-flow-map.md) | The hub README now owns goal-driven navigation directly, so the separate flow map duplicated hub content. | [`../README.md`](../README.md) "By Goal" section |
| [`031-docs-navigation-cleanup.md`](./031-docs-navigation-cleanup.md) | Historical record of the March 2026 navigation normalization. | No direct replacement |
| [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md) | Historical record of the 2026-03-01 sweep across primary entry points. | [`../039-docs-drift-monitoring.md`](../039-docs-drift-monitoring.md) |
| [`043-wrapper-channel-evaluation-and-policy.md`](./043-wrapper-channel-evaluation-and-policy.md) | The wrapper-channel decision settled on Phase E (no npm wrapper by default). Active distribution posture is covered directly in the distribution guides. | [`../041-distribution-ci-pinning-and-wrapper-migration.md`](../041-distribution-ci-pinning-and-wrapper-migration.md), [`../042-homebrew-tap-and-release-automation.md`](../042-homebrew-tap-and-release-automation.md) |
| [`053-release-wrapper-retirement-record-template.md`](./053-release-wrapper-retirement-record-template.md) | The compatibility-only release wrapper scripts have already been retired; this template is no longer an active procedure. | [`../051-release-orchestration.md`](../051-release-orchestration.md), [`../054-release-checkpoint-log-template.md`](../054-release-checkpoint-log-template.md) |

## Re-Promotion

If an archived guide becomes useful again, move it back out of `archive/`, strip
the deprecation header, and re-list it in the hub README. Record the move in
the next dated log under `docs/logs/YYYY-MM/` per policy 040 §5.
