# g08.001 - Graph-Aware Scan Intelligence Suite

Status: Complete
Depends on: `g07.078`

## Goal

Use the existing graph index to make `effigy scan` more useful for agents and
maintainers, without making scan depend on graph availability.

The current scan surface is intentionally filesystem-first: god files,
duplicate blocks, comment ratio, generated assets, generated-in-src, attention
markers, and stale suppressions. Those checks are valuable because they are
cheap and predictable, but they cannot answer relationship questions.

`g08` adds that missing relationship layer.

## Scope

- define how scan commands discover and report graph readiness
- enrich existing scan findings when a ready graph is available
- add graph-native scans for boundaries, isolated code, and validation gaps
- keep graph-backed results additive and explicit in JSON contracts
- prove behavior on fixture repos plus at least one non-Effigy repo shape

## Guardrails

- do not make existing scans require a graph index
- do not silently run indexing from scan commands
- do not hide stale graph state
- do not introduce Effigy-only path, crate, or task assumptions
- do not treat graph heuristics as compiler truth
- do not break existing JSON consumers

## Ordered Lanes

1. [`002-scan-graph-contract-and-readiness-model.md`](./002-scan-graph-contract-and-readiness-model.md)
2. [`003-existing-scan-graph-enrichment.md`](./003-existing-scan-graph-enrichment.md)
3. [`004-boundary-and-layer-violation-scans.md`](./004-boundary-and-layer-violation-scans.md)
4. [`005-dead-and-isolated-code-scans.md`](./005-dead-and-isolated-code-scans.md)
5. [`006-validation-gap-and-hotspot-scans.md`](./006-validation-gap-and-hotspot-scans.md)
6. [`007-agent-docs-json-and-benchmark-proof.md`](./007-agent-docs-json-and-benchmark-proof.md)
7. [`008-graph-aware-scan-closeout.md`](./008-graph-aware-scan-closeout.md)

## Acceptance Criteria

- graph-backed scan behavior is gated by an explicit readiness model
- existing scans remain fast and useful without a graph index
- enriched findings explain which graph facts changed the severity or context
- new graph-native scans produce repo-agnostic, test-backed results
- docs and the Effigy skill explain when agents should use graph-aware scans

## Next Task

Start `g08.002`.
