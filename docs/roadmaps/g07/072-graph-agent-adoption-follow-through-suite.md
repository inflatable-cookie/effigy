# g07.072 - Graph Agent Adoption Follow-Through Suite

Status: Complete
Depends on: `g07.071`

## Goal

Make `effigy graph` become the natural first navigation move for agents in any
Effigy-adopting repo, not just in the Effigy codebase.

The current graph is useful, but the live audit showed that agents still fall
back to `rg` too often because freshness is not invisible enough, behavioral
queries are sensitive to wording, and some packets land near the owner rather
than on the edit target.

## Evidence From The Audit

Strong cases:

- `where is catalog discovery implemented` found
  `crates/effigy-routing/src/discovery.rs`
- `where is task listing rendered` found
  `crates/effigy-tasks/src/listing.rs`
- `where is workspace linux artifact handoff implemented` found
  `src/runner/system_command/workspace_provisioning.rs`
- `where is graph explore ranking implemented` found
  `crates/effigy-codegraph/src/query/mod.rs`

Weak or mixed cases:

- `where is the init wizard setup inventory built` landed near the split owner
  but did not clearly identify inventory ownership first
- `where does effigy prompt to shut containers down on shell exit` missed until
  the query was rephrased as `prompt container shutdown on shell exit`
- reindexing was required before trust was clear, and the user-facing status
  model still takes attention away from the actual coding question

## Scope

- make graph freshness and trust cheaper for agents to reason about
- improve behavioral-query ranking without hard-coding Effigy-only phrases
- improve edit-target and test-target packets for split features
- add a cross-repo adoption benchmark that includes Effigy plus non-Effigy
  repos using different languages and project shapes
- update the agent skill and docs with concrete query shapes that are useful
  across projects
- close with evidence that graph reduces navigation work on real tasks

## Non-Goals

- no MCP server
- no graph daemon
- no JavaScript dependency
- no Effigy-only synonym table that cannot generalize to other repos
- no private benchmark that only runs against this repository
- no claim that `graph` replaces exact-token `rg` proof

## Ordered Lanes

1. [`073-graph-freshness-trust-and-cross-repo-readiness.md`](./073-graph-freshness-trust-and-cross-repo-readiness.md)
2. [`074-behavioral-query-ranking-and-vocabulary.md`](./074-behavioral-query-ranking-and-vocabulary.md)
3. [`075-edit-target-and-related-test-packets.md`](./075-edit-target-and-related-test-packets.md)
4. [`076-cross-repo-agent-usage-benchmark.md`](./076-cross-repo-agent-usage-benchmark.md)
5. [`077-agent-skill-and-doc-query-guidance.md`](./077-agent-skill-and-doc-query-guidance.md)
6. [`078-graph-agent-adoption-closeout.md`](./078-graph-agent-adoption-closeout.md)

## Acceptance Criteria

- graph status/explore gives agents a clear freshness signal without broad
  manual interpretation
- behavioral queries improve on at least three repos, not only Effigy
- explore packets identify likely edit files and likely tests more directly
- benchmark cases record graph-vs-search evidence without inflated marketing
  claims
- the Effigy skill teaches when graph should be first and when `rg` is still
  the correct final proof

## Batch Cards

- [`1022-open-graph-agent-adoption-lane.md`](./batch-cards/1022-open-graph-agent-adoption-lane.md)
- [`1023-tighten-graph-freshness-trust-model.md`](./batch-cards/1023-tighten-graph-freshness-trust-model.md)
- [`1024-improve-behavioral-query-ranking.md`](./batch-cards/1024-improve-behavioral-query-ranking.md)
- [`1025-add-edit-target-and-test-packet-proof.md`](./batch-cards/1025-add-edit-target-and-test-packet-proof.md)
- [`1026-build-cross-repo-agent-usage-benchmark.md`](./batch-cards/1026-build-cross-repo-agent-usage-benchmark.md)
- [`1027-update-agent-guidance-for-graph-adoption.md`](./batch-cards/1027-update-agent-guidance-for-graph-adoption.md)
- [`1028-close-graph-agent-adoption-lane.md`](./batch-cards/1028-close-graph-agent-adoption-lane.md)

## Next Task

Start `1022`.
