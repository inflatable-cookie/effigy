# g09 Roadmaps

Status: Active
Theme: Operator command-surface clarity

## Purpose

`g09` tests command-surface compaction in live use, then keeps only the part that
helps: grouped discovery with direct execution.

## Roadmaps

- [`001-command-surface-compaction-preview.md`](./001-command-surface-compaction-preview.md)
- [`002-flat-command-execution.md`](./002-flat-command-execution.md)

## Design Posture

- one command implementation and one output owner per operation
- help groups organize discovery; direct built-ins are canonical execution
- task selectors and slash catalog selectors stay deterministic
- no silent break, removal, release, or consumer rewrite
- structured machine warnings never contaminate JSON stdout

## Current State

[`g09.001`](./001-command-surface-compaction-preview.md) is complete historical
evidence. [`g09.002`](./002-flat-command-execution.md) is active under strict
spec `117`; card
[`1110`](./batch-cards/1110-remove-executable-command-namespaces.md) is ready.

## Next Task

Execute ready card
[`1110`](./batch-cards/1110-remove-executable-command-namespaces.md). Effigy
release authority stays separate.
