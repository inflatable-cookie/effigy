# 1027 - Update Agent Guidance For Graph Adoption

Roadmap: [`../077-agent-skill-and-doc-query-guidance.md`](../077-agent-skill-and-doc-query-guidance.md)
Strict lane: [`../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-20

## Purpose

Teach agents when graph should be first, while keeping Effigy's broader task
surface visible.

## Work

- update project-local and distributed Effigy skills together
- update active graph guide and any first-contact docs
- add cross-repo query examples
- document fallback rules for `rg`
- keep deploy/state/distribution/container/task surfaces visible

## Guardrails

- graph is not the universal first command
- no Effigy-only examples as the only examples
- no edits to historical roadmaps as current guidance

## Acceptance

- skill guidance matches implemented behavior
- examples are portable across repo shapes
- docs checks pass

## Next Task

Move to `1028`.
