# 987 - Implement Traversal-Aware Explore

Roadmap: [`../038-traversal-aware-explore-assembly.md`](../038-traversal-aware-explore-assembly.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make `graph explore` assemble a bounded owner and relation chain instead of
only returning independently ranked files.

## Work

- seed traversal from top-ranked owners
- walk call, import/include, doc, manifest/task, and route edges with depth and
  byte limits
- classify output as primary, traversal neighbor, support, or fallback
- include relation reasons and confidence in JSON
- add tests for multi-hop flow questions
- rerun benchmark tasks that previously required follow-up `rg`

## Acceptance

- `explore` can explain why secondary files were included
- traversal is bounded and deterministic
- call-flow benchmark cases improve without output blow-up
- existing `context` behavior remains stable

## Evidence

- [`2026-05/18-154154-traversal-aware-explore.md`](../../../logs/2026-05/18-154154-traversal-aware-explore.md)

## Next Task

Execute `988`.
