# g09 Roadmaps

Status: Active
Theme: Operator and consumer contract clarity

## Purpose

`g09` tests operator-facing changes in live use, keeps only what helps, and
extends the governed contract into current consumer evidence.

## Roadmaps

- [`001-command-surface-compaction-preview.md`](./001-command-surface-compaction-preview.md)
- [`002-flat-command-execution.md`](./002-flat-command-execution.md)
- [`003-acowtancy-consumer-adoption-replay.md`](./003-acowtancy-consumer-adoption-replay.md)

## Design Posture

- one command implementation and one output owner per operation
- help groups organize discovery; direct built-ins are canonical execution
- task selectors and slash catalog selectors stay deterministic
- no silent break, removal, release, or consumer rewrite
- structured machine warnings never contaminate JSON stdout
- consumer replays preserve repository ownership and exact revision identity

## Current State

[`g09.001`](./001-command-surface-compaction-preview.md) and
[`g09.002`](./002-flat-command-execution.md) are complete historical evidence.
Direct invocation is canonical; help grouping remains.

[`g09.003`](./003-acowtancy-consumer-adoption-replay.md) is active under strict
spec `118`. Card `1111` executed the frozen, read-only Acowtancy consumer
contract replay and published the first populated comparison scorecard; its
PR awaits orchestrator exact-head review.

## Next Task

After the card `1111` PR merges, one planning checkpoint decides cohort
expansion. Effigy release authority stays separate.
