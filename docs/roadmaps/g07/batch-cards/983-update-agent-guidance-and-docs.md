# 983 - Update Agent Guidance And Docs

Roadmap: [`../033-agent-guidance-and-skill-update.md`](../033-agent-guidance-and-skill-update.md)
Strict lane: [`../../../specs/090-graph-explore-agent-navigation-strict-lane.md`](../../../specs/090-graph-explore-agent-navigation-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make the new graph exploration workflow easy for agents to find and use.

## Work

- update graph guide docs
- update command reference coverage
- update rustdoc for the graph query surface
- update `skills/effigy/SKILL.md`
- keep exact-search fallback guidance visible
- document watch/index freshness expectations

## Acceptance

- docs and skill teach the same workflow
- path and link checks pass
- no docs claim that `graph explore` replaces `rg`

## Evidence

- [`2026-05/18-133020-graph-explore-implementation-closeout.md`](../../../logs/2026-05/18-133020-graph-explore-implementation-closeout.md)

## Next Task

Execute `984`.
