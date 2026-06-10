# Agent Context Packs

Date: 2026-05-18  
Roadmap: [`g07.011`](../roadmaps/g07/011-agent-context-packs.md)  
Batch card: [`911`](../roadmaps/g07/batch-cards/911-implement-agent-context-packs.md)  
Strict lane: [`085`](../specs/085-code-graph-intelligence-strict-lane.md)

## What Changed

- replaced the old `graph context` payload shape with bounded `items` plus
  explicit `overflow` accounting
- added per-item rank, score, reasons, provenance, optional ranges, and bounded
  snippets
- added stale-state note propagation into context packs
- updated text rendering so non-JSON callers see rank, score, reasons, snippet
  truncation, and overflow counts
- added byte-budget and truncation regression coverage

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo check -p effigy-cli -p effigy`
- `cargo fmt --all -- --check`

## Remaining Limits

- ranking is still lexical and graph-neighborhood based; no semantic or type
  resolution
- snippet budgeting is byte-based and intentionally conservative
- context packs do not yet carry benchmark evidence; that closes in `912`

## Vision Target Delta

- primary vision tags touched: `ROUTE`, `CONTRACT`, `OPERATE`
- moved: `graph context` from thin file/symbol listing to bounded, reasoned
  agent context packs with explicit overflow accounting and snippet limits
- remains open: `g07.012` performance, cache, and regression closeout
