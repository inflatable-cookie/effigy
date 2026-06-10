# 982 - Implement Graph Explore Command

Roadmap: [`../032-explore-context-assembly-command.md`](../032-explore-context-assembly-command.md)
Strict lane: [`../../../specs/090-graph-explore-agent-navigation-strict-lane.md`](../../../specs/090-graph-explore-agent-navigation-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Add `effigy graph explore "<question>"` with bounded excerpts, ranked owners,
related graph facts, and JSON output.

## Work

- add CLI parsing and help coverage
- add query assembly in `effigy-codegraph`
- add text and JSON rendering
- add tests for parsing, JSON shape, excerpt bounds, and benchmark-target
  queries
- preserve compatibility for every existing graph command

## Acceptance

- `effigy graph explore --json` returns the planned contract
- text output includes provenance and ranges
- focused graph and CLI tests pass
- no existing graph command regresses

## Evidence

- [`2026-05/18-133020-graph-explore-implementation-closeout.md`](../../../logs/archive/2026-05/18-133020-graph-explore-implementation-closeout.md)

## Next Task

Execute `983`.
