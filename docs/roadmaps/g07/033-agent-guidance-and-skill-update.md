# g07.033 - Agent Guidance And Skill Update

Status: Complete
Depends on: `g07.032`

## Goal

Teach agents to use `graph explore` as the first navigation tool without
overclaiming what it can do.

## Scope

- update graph guide docs
- update command reference coverage
- update rustdoc for the graph query surface
- update bundled `skills/effigy/SKILL.md`
- update any graph workflow examples that still start with broad `rg`
- document exact-match fallback behavior
- document watch-mode warm-up for long sessions

## Agent Rule Target

The guidance should be blunt:

- use `effigy graph explore "<task-shaped question>" --json` first for codebase
  navigation
- trust returned excerpts for first-pass orientation
- do not reread files already covered by excerpts unless the excerpt is
  insufficient for the edit or review
- use `rg` for exact token lookup, missing symbols, and verification before
  editing
- run `effigy graph watch` or `effigy graph index` when the graph is stale

## Guardrails

- no MCP claims
- no "replaces filesystem search" claim
- no benchmark claims until `984` records evidence
- keep examples generic enough for consumer repos
- keep local Effigy development examples clearly labelled as examples

## Acceptance Criteria

- docs and skill teach the same workflow
- guide links pass path and link checks
- agents can discover `graph explore` from the skill without reading the full
  docs tree

## Evidence

- [`2026-05/18-133020-graph-explore-implementation-closeout.md`](../../logs/2026-05/18-133020-graph-explore-implementation-closeout.md)

## Next Task

Execute `984`.
