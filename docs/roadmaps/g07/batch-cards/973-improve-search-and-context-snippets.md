# 973 - Improve Search And Context Snippets

Roadmap: [`../025-graph-context-ranking-quality-suite.md`](../025-graph-context-ranking-quality-suite.md)
Strict lane: [`../../../specs/089-graph-navigation-ranking-quality-strict-lane.md`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make graph output easier for agents to act on after ranking selects the right
records.

## Scope

- improve snippet placement around matched symbols or evidence spans
- keep byte budgets and overflow accounting intact
- make search results easier to choose from
- update docs/skill if command guidance changes

## Acceptance

- snippets are near matched evidence where possible
- search output has actionable path/name metadata
- docs still position `rg` as better for exact text
- evidence log exists:
  [`18-184500-graph-search-and-context-snippet-usefulness.md`](../../../logs/archive/2026-05/18-184500-graph-search-and-context-snippet-usefulness.md)

## Next Task

Execute `974`.
