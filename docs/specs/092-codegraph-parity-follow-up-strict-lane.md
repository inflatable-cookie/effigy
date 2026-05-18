# 092 - CodeGraph Parity Follow Up Strict Lane

Roadmap: [`g07.046`](../roadmaps/g07/046-codegraph-parity-follow-up-suite.md)
Related planning:
- [`g07.047`](../roadmaps/g07/047-warm-query-latency-and-release-ranking.md)
- [`g07.048`](../roadmaps/g07/048-fixture-backed-parity-proof.md)
- [`g07.049`](../roadmaps/g07/049-codegraph-parity-follow-up-closeout.md)

Status: Paused
Owner: Platform
Created: 2026-05-18

## Purpose

Finish the bounded follow-up work left by the paused CodeGraph parity lane.

## Lane Posture

Posture: `paused-no-ready-card`

`091` closed with a clear result: graph usefulness is mostly there, but honest
parity is still blocked by warm-query latency and missing fixture-backed proof.
This lane exists to finish only those measured gaps.

## Hard Boundaries

- no reopening broad parity scope beyond the measured blockers
- no MCP server, daemon, JavaScript runtime, or plugin-runtime detour
- no hiding slow queries behind narrower benchmark wording
- no dropping reasons, provenance, freshness, or overflow evidence for speed
- no unsupported parity claim at closeout

## Execution Order

1. `996` complete: open the follow-up lane and currentness surfaces
2. `997` complete: reduce warm query latency and fix release ranking
3. `998` complete: add fixture-backed parity runner
4. `999` complete: close the follow-up lane

## Ready Chain

- `996` is complete.
- `997` is complete.
- `998` is complete.
- no current ready card remains.

## Auto-Continuation Envelope

Auto-start is enabled while:

- the previous card closes with benchmark or test evidence
- live-repo warm-query timings are measured against `g07.045`
- fixture-backed parity cases stay tied to the pinned gold query file
- public graph JSON contracts remain additive or unchanged

Stop and replan if:

- useful latency recovery requires a contract compromise worse than direct file
  reads
- fixture-backed parity proof needs a broader harness than this lane can own
- the remaining gap is clearly outside Effigy's intended operating model

## Acceptance

This lane is complete when:

- warm-query latency is either recovered or explicitly deferred with hard data
- the deferred parity cases are executable
- the final closeout makes an explicit parity posture call
- no active ready card remains

## Next Task

No active ready card.
