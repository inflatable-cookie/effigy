# 099 - Apple Containers Native Backend Prototype

Roadmap: [`g08.018`](../roadmaps/g08/018-apple-containers-native-backend-prototype.md)
Research: [`Translation Memo 017`](../research/translation-memos/017-apple-containers-runtime-backend.md)

Status: Paused — prototype complete, watch-only decision
Owner: Platform
Created: 2026-08-01

## Purpose

Control the Apple Containers prototype so native runtime work cannot widen into
an undocumented partial Compose engine or silently change current backend
selection.

## Lane Posture

Posture: `strict-paused`

User approval on 2026-08-01 opens the prototype. Only the current ready card may
execute; later cards remain queued until their predecessor closes with evidence.

## Hard Boundaries

- no public support claim before Batch C
- no automatic Apple backend detection during the prototype
- no arbitrary Compose-file translation
- no ignored or lossy Compose fields
- no machine-global DNS dependency for normal stack operation
- no Docker/Colima behavior regression
- no release mutation or `.github/workflows/` edit
- no destructive Apple runtime cleanup outside Effigy-created prototype names

## Execution Order

1. `1051` — signed runtime baseline and typed effective stack plan
2. `1052` — native adapter and representative stack lifecycle
3. `1053` — parity/resource proof and support decision

## Ready Chain

- `1051` is complete
- `1052` is complete
- `1053` is complete
- no execution card is ready

## Stop Conditions

Stop and replan if:

- the signed Apple installer cannot be verified or requires unavailable
  operator interaction
- catalog services cannot be represented without turning Compose YAML into the
  durable semantic API
- service discovery requires Apple system DNS or another machine-global
  mutation
- representative idle memory or I/O is clearly impractical before the full
  matrix is complete
- native support requires weakening current container, gateway, data, secret,
  or cleanup contracts
- a stable public manifest change becomes necessary before prototype evidence
  exists

## Acceptance

The lane closes only when cards `1051` through `1053` are complete or the
roadmap records a deliberate pause/reject decision with reproducible evidence.

## Next Task

Keep the lane paused. Replan only when guide `077`'s boot-time discovery
reassessment gate is met.
