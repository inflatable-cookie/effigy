# Graph Scan Cost Closeout

Date: 2026-05-18  
Roadmap: [`g07.017`](../roadmaps/g07/017-graph-scan-cost-reduction-suite.md)  
Batch card: [`954`](../roadmaps/g07/batch-cards/954-close-graph-scan-cost-proof.md)  
Strict lane: [`087`](../specs/087-graph-scan-cost-reduction-strict-lane.md)

## What Changed

- closed the bounded graph scan-cost reduction lane
- measured the raw walk floor directly
- removed the duplicate no-op scan inside `graph index`
- collapsed `graph status` path classification to one repo scan snapshot

## Measured Delta

Compared with the `g07.013` closeout baseline:

- no-op `graph index --json`
  - baseline: `17.71s`
  - after `g07.017`: `0.25s`
- `graph status --json`
  - baseline: `0.48s`
  - after `g07.017`: `0.21s` to `0.24s`

Direct scan-floor evidence from `951`:

- walk only: `50ms`
- walk + graph filtering: `43ms`
- walk + filtering + metadata: `50ms`

## Conclusion

- the remaining scan-path duplication was worth fixing
- the raw repo walk is now cheap enough that further scan-only work is likely
  to return marginal wins
- another graph performance lane should only open if you are willing to take on
  riskier machinery such as stronger persistent scan caches or watcher-backed
  invalidation

That is not justified right now.

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo build --bin effigy`
- clean repeated `./target/debug/effigy graph status --json`
- clean `./target/debug/effigy graph index --json`
- `./target/debug/effigy docs check paths ...`
- `git diff --check`

## Residual Limits

- no-op graph commands still pay for one repo walk and metadata collection
- lexical search tools like `rg` remain a better fit for raw text lookup than
  the graph
- larger wins now likely need a different architecture, not another small pass

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: scan-cost suspicion -> direct floor measurement plus bounded scan-path
  reductions with stable low-latency no-op graph commands
- remains open: no active `g07` execution card
