# 037 — Documentation contribution

Update documentation in the same PR as the behavior it describes. The [knowledge index](../knowledge/README.md) names one owner per current fact; guides teach users how to apply that fact.

For command changes, update the [command reference](025-command-reference-matrix.md), a relevant workflow guide, and troubleshooting if errors change. For JSON changes, update the [JSON guide](017-json-output-contracts.md), examples, and owning schema. For manifest changes, update the [manifest cookbook](022-manifest-cookbook.md). For release changes, update the [release procedure](../knowledge/contracts/release.md) and relevant distribution guide.

Keep `README.md`, `docs/README.md`, and `docs/guides/README.md` navigable. Move retired product facts into [retired concepts](../knowledge/retired.toml) and fix live references. Do not create repository task status or delivery logs. Run `effigy qa:docs`, then `effigy qa` before the PR.
