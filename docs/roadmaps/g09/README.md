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

[`g09.001`](./001-command-surface-compaction-preview.md) is complete: card
[`1109`](./batch-cards/1109-add-executable-command-namespaces.md) shipped the
additive preview and strict spec `116` is archived. No implementation lane is
active.

## Next Task

Await the future `v1.0` consumer-evidence checkpoint: direct-route removal
requires a refreshed consumer inventory plus explicit release authority, and
no removal card is readied before that gate. Effigy release authority stays
separate.
