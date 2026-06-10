# Code Graph Intelligence Lane Opened

Date: 2026-05-17

## Summary

Opened `g07.001` as the active native code graph intelligence lane.

The lane is scoped to deterministic local graph artifacts exposed through
Effigy CLI JSON. It explicitly excludes MCP, daemon mode, external language
plugins, JavaScript runtime dependencies, and LLM-generated summaries as
canonical graph data.

## What Changed

- Added strict lane
  [`085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md).
- Added batch cards `900` through `912` under
  [`docs/roadmaps/g07/batch-cards`](../../../roadmaps/g07/batch-cards/900-open-code-graph-intelligence-lane.md).
- Marked `g07.001` active.
- Marked `900` complete and `901` ready.
- Refreshed roadmap and spec front doors.

## Vision Target Delta

- `CONTRACT`: graph work starts from strict JSON contracts and provenance rules.
- `OPERATE`: the feature stays CLI-first and local.
- `MAINT`: v1 stays first-party and avoids plugin/server/runtime management.

Baseline -> current state:

- baseline: `g07` planned with no active execution slice.
- current: strict lane `085` active, `901` ready.

What remains open:

- all implementation work from `901` through `912`.

## Validation

- Pending docs path validation in the opening batch.

## Next Task

Execute `901`.
