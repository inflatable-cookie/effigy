# Vision Decision Record — D-2026-04

Context
- Date: 2026-08-29
- Owner: Operator + Platform
- Scope: documentation retrieval across Effigy consumer repositories
- Tags: OPERATE, MAINT, ROUTE, CONTRACT

Decision
- Summary: Select Horizon Theme 2 as a narrow `g08` documentation-graph
  extension with repository-owned profiles and Northstar as one committed
  template.
- Principle(s): explicitness over implicit fallback (`008`); keep docs and
  contracts synchronized (`020`).
- Chosen Option: reuse the native graph for exact Markdown structure and
  bounded retrieval; read semantic shape only from the consumer `effigy.toml`.

Alternatives Considered
- Option A: hard-code the Northstar documentation spine — rejected because
  non-Northstar repositories need their own vocabulary and authority model.
- Option B: load semantics directly from an installed Northstar skill —
  rejected because skill updates would silently reinterpret committed repos.
- Option C: build a separate knowledge-graph service — rejected because the
  existing graph already owns storage, freshness, FTS, and traversal.

Impact
- Positive: agents can retrieve current authoritative documentation in bounded
  context while repositories retain control of their own shape.
- Risk: profile grammar and ranking could grow into a second policy engine or
  duplicate existing graph ownership.
- Compatibility Effect: additive; baseline mode remains available without a
  profile.

Controls
- Mitigation: architecture `024`, contract `041`, strict spec `108`, one shared
  graph store, generic fixtures, explicit budgets, and no generated summaries.
- Reversal Condition: generic repositories require Northstar branches, a second
  graph store becomes authoritative, or profile scoring injects unrelated
  evidence.
- Exit Plan: close cards `1088` through `1090` with generic and Northstar proof,
  or return the lane to planning with the failed contract assumption named.

Traceability
- Related Exception: none
- Related Risk: VR-04
- Related Artifacts: [`architecture 024`](../../architecture/024-repository-defined-documentation-graph.md), [`contract 041`](../../contracts/041-documentation-graph-profile-contract.md), [`g08.035`](../../roadmaps/g08/035-repository-defined-documentation-graph.md), [`strict spec 108`](../../specs/archive/108-documentation-graph-profiles-strict-lane.md)

Review checkpoint: lane `g08.035` closeout or 2026-09-17, whichever comes first.
