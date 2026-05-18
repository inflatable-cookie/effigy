# g07.041 - Source Section Packets And No-Reread Workflow

Status: Complete
Depends on: `g07.040`

## Goal

Make `graph explore` return source sections that are complete enough for
first-pass agent reasoning without opening the same files immediately.

## Scope

- define "source section" boundaries per language:
  - function/method/class
  - adjacent impl block or type definition
  - route declaration plus handler
  - manifest task plus referenced script/task
  - markdown heading section
- return section completeness metadata:
  - complete section
  - truncated section
  - surrounding context only
  - relation-only item
- tune byte budgets by role rather than flat per-item limits
- add overflow guidance that lists what was omitted and why
- support a compact text mode for humans and a stable JSON mode for agents
- update docs/skill to state the no-reread rule precisely

## Guardrails

- no payload explosion
- no hiding truncation
- no promising edit readiness when a section is incomplete
- no removal of `rg` guidance for exact token verification

## Acceptance Criteria

- benchmark tasks record fewer rereads of returned files
- excerpts include enough local context for normal first-pass explanation
- incomplete packets tell agents exactly what to open next
- JSON examples cover complete, truncated, and overflow shapes

## Evidence

- [`2026-05/18-160609-source-section-packets.md`](../logs/2026-05/18-160609-source-section-packets.md)

## Next Task

Execute `991` after section packets are good enough for zero-reread benchmark
measurement.
