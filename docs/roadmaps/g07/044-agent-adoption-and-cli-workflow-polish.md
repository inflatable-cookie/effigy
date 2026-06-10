# g07.044 - Agent Adoption And CLI Workflow Polish

Status: Complete
Depends on: `g07.043`

## Goal

Make the graph workflow obvious and cheap for agents across Codex, Claude Code,
Cursor, OpenCode, and humans using the CLI.

## Scope

- update `skills/effigy/SKILL.md` with final graph-first workflow
- update docs/guides, command reference, JSON examples, and rustdoc
- add task aliases or Effigy repo QA tasks only if they reduce real friction
- improve command help for:
  - `graph status`
  - `graph explore`
  - `graph affected`
  - watch/freshness remediation
- add examples for:
  - first graph call in a fresh repo
  - stale index recovery
  - exact-match fallback to `rg`
  - no-reread source-section use
  - affected-test narrowing
- decide whether a non-MCP "agent prompt snippet" command is useful

## Guardrails

- no MCP-specific requirement
- no global installer behavior in core unless explicitly planned
- no docs that tell agents to trust stale graph output
- no overclaiming parity before closeout metrics pass

## Acceptance Criteria

- an agent entering an Effigy repo can discover the graph path from the skill
  and docs without rereading internal roadmaps
- help text points to the right commands
- JSON examples match actual output
- docs preserve exact-search and stale-index warnings

## Evidence

- [`2026-05/18-173200-agent-adoption-and-cli-workflow-polish.md`](../../logs/archive/2026-05/18-173200-agent-adoption-and-cli-workflow-polish.md)

## Next Task

Execute `994` after the adoption surface is aligned with implementation.
