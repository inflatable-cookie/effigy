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
- [`004-release-gate-diagnosability.md`](./004-release-gate-diagnosability.md)
- [`005-docs-context-latency-and-freshness.md`](./005-docs-context-latency-and-freshness.md)
- [`006-cross-repository-source-routing.md`](./006-cross-repository-source-routing.md)

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

[`g09.003`](./003-acowtancy-consumer-adoption-replay.md) is complete under
strict spec `118`. Card `1111` produced the frozen, read-only Acowtancy replay
and first populated comparison scorecard; PR `88` merged at `9c05a883`.

[`g09.004`](./004-release-gate-diagnosability.md) is ready under strict spec
`119`: persist release gate output and environment, show the failing tail,
announce the gate inventory. Card `1112` is the ready card.

[`g09.005`](./005-docs-context-latency-and-freshness.md) is queued under
strict spec `120` (card `1113`, serial after `1112`): reproduce and repair
`docs context` warm and stale latency to frozen budgets.
[`g09.006`](./006-cross-repository-source-routing.md) is planned and gated on
that evidence; it has no ready card.

## Next Task

Execute card `1112`, then card `1113`. The consumer cohort checkpoint is deferred by operator
direction (2026-09-05). Effigy release authority stays separate.
