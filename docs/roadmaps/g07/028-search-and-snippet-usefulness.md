# g07.028 - Search And Snippet Usefulness

Status: Complete
Depends on: `g07.027`

## Goal

Improve the agent-facing usefulness of `graph search` and context snippets
without pretending they replace direct file reads.

## Scope

- make `graph search` more useful for graph records:
  - include clearer names and paths
  - prefer symbols/files over incidental docs when appropriate
  - expose enough metadata for agents to choose the next query
- improve `graph context` snippets:
  - start near the best matching symbol or reason-bearing span when possible
  - fall back to file start only when no better evidence exists
  - preserve byte budget and truncation accounting
- add tests for snippet location, not only payload shape
- update docs/skill guidance if command positioning changes

## Guardrails

- do not make snippets unbounded
- do not read entire repos into output
- do not replace source reads; context packs remain a bounded starting point
- keep JSON contract additive if new fields are needed

## Acceptance Criteria

- context snippets for symbol-driven matches start near matching code
- `graph search` results are easier to act on than raw record IDs
- docs continue to say `rg` is better for exact text

## Next Task

After `973`, execute `974`.
