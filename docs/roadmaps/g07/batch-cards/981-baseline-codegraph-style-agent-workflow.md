# 981 - Baseline CodeGraph-Style Agent Workflow

Roadmap: [`../031-explore-contract-and-benchmark-baseline.md`](../031-explore-contract-and-benchmark-baseline.md)
Strict lane: [`../../../specs/090-graph-explore-agent-navigation-strict-lane.md`](../../../specs/090-graph-explore-agent-navigation-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Measure the current agent navigation workflow before implementing
`graph explore`.

## Work

- define the `graph explore` text and JSON contract
- run 5 to 8 benchmark tasks through the current workflow
- count graph calls, file reads, `rg` calls, and elapsed time
- record where current `graph context` already avoids broad scanning
- record where agents still need immediate follow-up reads
- produce a log that becomes the acceptance target for `982`

## Acceptance

- baseline log exists under `docs/logs/archive/2026-05/`
- contract sketch is explicit enough to implement
- benchmark tasks and metrics are stable
- `982` is ready with concrete implementation targets

## Evidence

- [`2026-05/18-132133-graph-explore-baseline.md`](../../../logs/archive/2026-05/18-132133-graph-explore-baseline.md)

## Next Task

Execute `982`.
