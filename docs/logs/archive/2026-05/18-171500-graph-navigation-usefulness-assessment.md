# Graph Navigation Usefulness Assessment

Date: 2026-05-18  
Roadmap: [`g07.025`](../../../roadmaps/g07/025-graph-context-ranking-quality-suite.md)  
Strict lane: [`089`](../../../specs/089-graph-navigation-ranking-quality-strict-lane.md)

## What Changed

- assessed current graph usefulness against direct filesystem search
- identified where `graph context` already helps and where it does not
- opened the next bounded ranking-quality lane

## Evidence

Representative local timings:

- no-op `graph index --json`: `0.29s`
- `graph search release --limit 5 --json`: `0.42s`
- `graph context "trace release orchestrator" --language rust --max-files 6 --max-bytes 2048 --json`: `2.13s`
- `rg -n "release"`: `0.06s`

Representative quality observations:

- `graph context "trace deploy provider export"` returned the right deploy
  owner files first, including `provider_package.rs` and deploy transaction
  modules.
- `graph context "trace graph watch implementation"` over-ranked
  `crates/effigy-codegraph/src/tests.rs` and broad graph files before the
  actual watch implementation.
- `graph search release` returned graph records, but direct `rg` was faster and
  more appropriate for exact token lookup.

## Conclusion

The graph is useful now as a bounded repo-map and context pack generator. It
does not replace direct filesystem search.

The recommended workflow remains:

1. use `graph context` for a first owned-file set
2. use `graph node`, `callers`, `callees`, or `impact` after a relevant record
   is known
3. use `rg` for exact text lookup and verification

## Follow-Up Plan

Open `g07.025` to improve:

- gold-task ranking tests
- role-aware ranking
- generic-token handling
- repeated-symbol score caps
- context snippet placement
- search result actionability

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: graph usefulness from anecdotal status to measured ranking-quality lane
- remains open: execute `971` through `974`
