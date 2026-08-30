# g08.035 - Repository-Defined Documentation Graph

Status: Paused
Depends on: completed native code graph and graph agent-adoption lanes
Spec: [`108`](../../specs/108-documentation-graph-profiles-strict-lane.md)
Architecture: [`024`](../../architecture/024-repository-defined-documentation-graph.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)

## Goal

Let any repository describe its documentation semantics in `effigy.toml`, then
retrieve small, current, authoritative source sections through the existing
graph. Ship Northstar as one profile without coupling the generic runtime to
Northstar files or skills.

## Vision Alignment

- Primary tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Target envelope: measured reduction in agent documentation-discovery thrash
  on Effigy and one repository-neutral fixture.
- Vision target delta: the selected agent-native maintainer theme gains a
  documentation-specific graph layer with repository-owned semantics.

## Execution Plan

- [x] card `1088`: build typed repository profiles, exact sections, fields,
      relations, and profile-aware freshness
- [ ] card `1089`: add bounded `effigy docs context` retrieval, CLI/help, and
      versioned JSON after card `1091` closes the overlapping maintenance lane
- [ ] card `1090`: prove generic and Northstar profiles, publish adoption
      guidance, benchmark retrieval, validate, and close the lane

## Owner And Seam

The existing graph remains storage and freshness authority. Manifest code owns
configuration grammar; codegraph owns semantics and retrieval; CLI and built-in
docs code own only routing and rendering. Northstar starters may author a
profile into a repository but are never consulted at runtime.

## Non-Goals

- no Northstar-only runtime rules
- no second graph database, daemon, or required MCP server
- no embeddings or remote inference in this lane
- no generated summaries as canonical graph data
- no external documentation crawl
- no release or CI workflow mutation

## Acceptance

- [ ] repositories with and without profiles both get useful docs context
- [ ] arbitrary repository vocabularies work without code changes
- [ ] exact section evidence includes path, span, facts, currentness, authority,
      relation path, and match reason
- [ ] output budgets and deterministic ordering hold in text and JSON
- [ ] profile changes participate in graph freshness
- [ ] the Northstar starter is committed configuration, not runtime inheritance
- [ ] a benchmark corpus returns the expected live authority within the top
      three results and does not rank a historical-only counterpart above it
- [ ] lane closeout records focused and full validation evidence

## Next Task

Execute ready maintenance card
[`1091`](./batch-cards/1091-audit-and-refresh-documentation-instructions-and-help.md).
Its closeout resumes this roadmap at card `1089`.
