# g07.038 - Traversal-Aware Explore Assembly

Status: Complete
Depends on: `g07.037`

## Goal

Make `graph explore` assemble context by traversing graph topology, not only by
ranking independent files.

## Scope

- add bounded traversal from top-ranked seeds across:
  - call/callee edges
  - import/include edges
  - doc path references
  - manifest task references
  - route/entrypoint edges once `g07.040` lands
- return traversal reasons in JSON so agents can see why a secondary file is
  present
- cap traversal by depth, byte budget, file budget, and edge kind
- prefer owner chains over same-directory filler
- distinguish:
  - primary owners
  - traversal neighbors
  - supporting docs/config
  - exact-search fallbacks
- add tests for multi-hop flows such as request entrypoint to handler to lower
  service/helper

## Guardrails

- no unbounded graph walk
- no hidden filesystem scans while traversing
- no output that hides whether a relation is exact, syntactic, or heuristic
- no traversal claim when the extractor did not emit the needed edge

## Acceptance Criteria

- `graph explore` can show a short flow chain, not just a list of likely files
- benchmark architecture/call-flow questions need fewer follow-up `rg` calls
- traversal output remains deterministic and bounded
- JSON remains additive or versioned

## Evidence

- [`2026-05/18-154154-traversal-aware-explore.md`](../logs/2026-05/18-154154-traversal-aware-explore.md)

## Next Task

Execute `988` to widen extractor coverage so more language families emit
high-value traversal edges.
