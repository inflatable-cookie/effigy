# 911 - Implement Agent Context Packs

Roadmap: [`../011-agent-context-packs.md`](../011-agent-context-packs.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Add bounded, deterministic context packs for agents.

## Scope

- `effigy graph context "<task>" --json`
- rank likely relevant files, symbols, docs, tasks, and manifests
- include bounded snippets
- include selection reasons
- include omitted/overflow counts
- support max files, max bytes, language filters, and path filters

## Guardrails

- no generated implementation plan
- no LLM scoring in v1
- no unbounded snippets
- no hidden broad scans outside graph policy

## Acceptance

- output is small enough for agent prompts
- every selected item explains why it was selected
- output is deterministic for the same graph and query

## Next Task

Execute `912`.
