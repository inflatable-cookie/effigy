# 993 - Polish Agent Adoption And CLI Workflow

Roadmap: [`../044-agent-adoption-and-cli-workflow-polish.md`](../044-agent-adoption-and-cli-workflow-polish.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Make the finished graph workflow easy for agents and humans to discover and
use correctly.

## Work

- update the Effigy skill graph section
- update guide, command reference, JSON examples, and rustdoc
- improve graph command help text
- add examples for fresh index, stale recovery, exact-match fallback,
  no-reread source sections, and affected-test narrowing
- decide whether a CLI prompt-snippet command is useful without adding MCP

## Acceptance

- agents can discover the graph-first workflow from the skill and docs
- command help is accurate
- JSON examples match current payloads
- docs retain stale-index and exact-search warnings

## Evidence

- [`2026-05/18-173200-agent-adoption-and-cli-workflow-polish.md`](../../../logs/archive/2026-05/18-173200-agent-adoption-and-cli-workflow-polish.md)

## Next Task

Execute `994`.
