# Graph Scan Cost Lane Opened

Date: 2026-05-18  
Roadmap: [`g07.017`](../roadmaps/g07/017-graph-scan-cost-reduction-suite.md)  
Batch card: [`950`](../roadmaps/g07/batch-cards/950-open-graph-scan-cost-lane.md)  
Strict lane: [`087`](../specs/087-graph-scan-cost-reduction-strict-lane.md)

## What Changed

- opened the bounded graph scan-cost reduction tranche
- kept the scope deliberately narrow:
  - file walk
  - scan metadata
  - repeated scan passes
- excluded watchers, daemons, platform-specific backends, and unsafe stale
  shortcuts

## Why This Lane Exists

`g07.013` fixed the large graph problems. What remains is a quality/perf floor:

- no-op `graph index --json`: `17.71s`
- `graph status --json`: `0.48s`

That is good enough to use, but still worth one short lane because the
remaining cost likely sits in scan behavior rather than extractor work.

## Immediate Queue

1. `951` baseline file-walk and scan cost
2. `952` reduce repeated scan work
3. `953` tighten stale/status scan cost
4. `954` closeout proof

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: no active graph card -> bounded scan-cost follow-up lane with strict
  non-goals
- remains open: `951` through `954`
