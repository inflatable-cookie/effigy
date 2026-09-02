# g09 Roadmaps

Status: Active
Theme: Operator command-surface compaction and explicit migration

## Purpose

`g09` opens with the operator-approved command-surface preview: preserve the
small daily task spine, make five job namespaces executable, and stage any
destructive cleanup behind explicit compatibility evidence.

## Roadmaps

- [`001-command-surface-compaction-preview.md`](./001-command-surface-compaction-preview.md)

## Design Posture

- one command implementation and one output owner per operation
- grouped routes are canonical; retained direct routes are migration aliases
- task selectors and slash catalog selectors stay deterministic
- no silent break, removal, release, or consumer rewrite
- structured machine warnings never contaminate JSON stdout

## Current State

Card [`1109`](./batch-cards/1109-add-executable-command-namespaces.md) is Ready.
It is the only current implementation lane.

## Next Task

Execute ready card [`1109`](./batch-cards/1109-add-executable-command-namespaces.md).
