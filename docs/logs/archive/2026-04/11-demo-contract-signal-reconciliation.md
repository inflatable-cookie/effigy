# Demo Contract Signal Reconciliation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.5`

## Summary

Reconciled Effigy's settled demo contract against Signal's live `demos/`
surface.

The result is that Signal validates the model direction, but also shows that
the orchestration layer is the real product gap:

- manifest-backed demo identity is working
- explicit scenario and receipt authority is working
- explicit coverage inventory is working
- the duplicated per-demo runner scripts are the unstable layer

## Directly Mappable Signal Concepts

- stable demo ids and human-facing titles/summaries
- repo-owned runnable entrypoints
- operator-notes/scenario references
- machine-readable receipts per attempt
- HTML companion views as artifacts
- explicit proof-coverage claims

## Runner-Owned Normalization Needed In Effigy

- shared lifecycle and gap vocabulary
- normalized receipt contract
- normalized latest-attempt and artifact summary for browser inspection
- registry-driven coverage/gap views instead of a separate handwritten matrix as
  the long-term source of truth

## Current Harness Debt

- one flat script runner per demo
- duplicated launch/render/receipt wiring
- project-local temporary serving and HTML generation in the runner layer
- separate coverage-matrix maintenance burden

## Follow-On

The next batch should not reopen the model. It should define the first bounded
implementation slice for demo registry loading, list/inspect surfaces, and
normalized latest-attempt state.
