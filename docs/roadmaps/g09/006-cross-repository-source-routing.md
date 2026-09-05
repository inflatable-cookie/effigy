# g09.006 Cross-Repository Source Routing

Status: Planned (conditional on `g09.005` evidence; no ready card)
Created: 2026-09-05
Depends on: [`g09.005`](./005-docs-context-latency-and-freshness.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Architecture: [`024`](../../architecture/024-repository-defined-documentation-graph.md)
Origin: Northstar shared-knowledge retrieval pilot,
`northstar/docs/triage/20260905-093742-shared-knowledge-retrieval-pilot.md`;
operator-confirmed direction relayed by the Northstar Chatterbox, 2026-09-05

## Purpose

Let an agent ask one question across an explicitly named set of local
repositories and get back exact sections with per-repository provenance,
using Effigy's existing repository-local retrieval underneath. Centralise
discovery, not copies of decisions.

## Gate

This roadmap compiles into a strict spec and ready card only after `g09.005`
closes with measured warm and stale budgets that make a multi-repository call
plausible inside an agent-sized budget. If `g09.005` shows that per-repository
retrieval cannot meet its budget, this roadmap is re-planned, not started.

## Fixed Direction (operator-confirmed)

- A source directory names allowed repositories, their documentation and
  skill roots, and canonical front doors. No implicit crawling of the
  projects directory; a repository absent from the directory is out of scope
  and reported as disallowed, not searched.
- Results retain repository identity, path, exact span, and commit or
  dirty-content identity. A working-tree excerpt is never labelled as exact
  commit bytes.
- Authority and currentness stay repository-declared. Per-repository authority
  weights are not compared as one global score; a repository with no profile
  reports unknown currentness and zero authority.
- Partial unavailability (missing checkout, stale or locked graph, timeout)
  is reported for that repository and must not block healthy repositories.
- Thin routing over existing local retrieval: no new index, daemon, MCP
  server, embeddings, hosted service, or artifact cache.
- No agent may promote retrieved content into another project's authority.

## To Freeze Before a Ready Card

- the caller surface (command shape, directory file location and grammar,
  `--json` payload id) and how it composes existing `docs context` budgets
- per-call latency and output budgets across N repositories, and the
  per-repository sub-budget derived from `g09.005` measurements
- partial-failure semantics and the exact per-repository status vocabulary
- access scope: how a directory entry is validated, what a disallowed or
  missing repository returns
- the replay protocol: the pilot's five frozen questions plus the required
  negative controls (missing source, retired versus current authority, same
  term in different repositories, dirty or stale checkout, disallowed
  repository, no relevant answer), each compared against ordinary source
  search for time to usable evidence, source correctness, and bytes returned

No speedup or recall claim is made before that replay is measured.

## Non-Goals

- central authoritative memory copies, embeddings, MCP server, hosted
  knowledge service, separate index or daemon, artifact caching
- agent notification or coordination changes
- release or workflow mutation; writes to any consumer repository
- Northstar documentation authority or lifecycle doctrine (Northstar owns it)

## Next Task

Wait for `g09.005` closeout evidence. Then Chatterbox freezes the items above
into a strict spec and one ready card, or re-plans.
