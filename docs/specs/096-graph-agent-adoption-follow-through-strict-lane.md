# 096 - Graph Agent Adoption Follow Through Strict Lane

Roadmap: [`g07.072`](../roadmaps/g07/072-graph-agent-adoption-follow-through-suite.md)
Related planning:
- [`g07.073`](../roadmaps/g07/073-graph-freshness-trust-and-cross-repo-readiness.md)
- [`g07.074`](../roadmaps/g07/074-behavioral-query-ranking-and-vocabulary.md)
- [`g07.075`](../roadmaps/g07/075-edit-target-and-related-test-packets.md)
- [`g07.076`](../roadmaps/g07/076-cross-repo-agent-usage-benchmark.md)
- [`g07.077`](../roadmaps/g07/077-agent-skill-and-doc-query-guidance.md)
- [`g07.078`](../roadmaps/g07/078-graph-agent-adoption-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-20

## Purpose

Make `effigy graph` a credible first navigation tool for agents across
Effigy-adopting repos.

The lane starts from a practical finding: the graph is useful, but agents still
default to `rg` because trust, phrasing, and edit-target precision are not yet
good enough.

## Hard Boundaries

- no MCP server
- no graph daemon
- no JavaScript runtime dependency
- no Effigy-only ranking hacks
- no private benchmark that only works on this repo
- no release mutation
- no `.github/workflows/` edits
- no claim that graph replaces exact-token search

## Cross-Repo Rule

Every behavior improvement must be designed for general repos first.

Effigy can be a fixture and a regression target, but a fix is not acceptable
when it only works because of Effigy-specific module names, paths, or task
vocabulary.

At minimum, proof must cover:

- Effigy
- one Underlay repo or fixture
- one decodelabs app/library or fixture
- one small synthetic fixture where external local repos are absent

Optional local repos may be skipped when absent, but the fixture-backed proof
must remain runnable.

## Execution Order

1. `1022`: open the lane and lock the adoption baseline
2. `1023`: tighten graph freshness trust signals
3. `1024`: improve behavioral-query ranking and vocabulary
4. `1025`: improve edit-target and test-target packets
5. `1026`: build the cross-repo usage benchmark
6. `1027`: update agent guidance without over-promoting graph
7. `1028`: close with measured proof and residual limits

## Ready Chain

- `1022` is complete
- `1023` is complete
- `1024` is complete
- `1025` is complete
- `1026` is complete
- `1027` is complete
- `1028` is complete
- later cards must not start until the prior card is complete or explicitly
  paused with a clear handoff

## Stop Conditions

Stop and replan if:

- a ranking change improves Effigy but fails generic fixtures
- JSON contract changes would break current consumers without migration
- freshness work starts auto-mutating indexes in surprising read-only contexts
- benchmark output starts making unsupported marketing claims
- skill guidance starts hiding non-graph Effigy surfaces

## Acceptance

This lane is complete when:

- all cards `1022` through `1028` are complete
- cross-repo benchmark evidence is recorded
- graph guidance is updated in skill and active docs
- remaining graph limits are stated plainly
- no active ready card remains

## Next Task

No active ready card.
