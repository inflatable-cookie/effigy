# CodeGraph Parity Closeout

Date: 2026-05-18  
Roadmap: [`g07.045`](../../../roadmaps/g07/045-codegraph-parity-closeout.md)  
Batch cards:
- [`994`](../../../roadmaps/g07/batch-cards/994-run-codegraph-parity-closeout.md)
- [`995`](../../../roadmaps/g07/batch-cards/995-close-or-rescope-codegraph-parity-lane.md)  
Strict lane: [`091`](../../../specs/091-codegraph-parity-strict-lane.md)

## Decision

Do **not** claim CodeGraph parity yet.

Current Effigy graph behavior is good enough on ranking quality and workflow
discoverability to be useful day to day, but it does **not** match CodeGraph on
warm-query speed for the live Effigy repo. This lane should park without an
active ready card and reopen only through a bounded follow-up plan focused on
query latency and fixture-backed parity proof.

## Fresh Benchmark Posture

Validated warm-index state after refresh:

- `graph index --json`: `14.49s`
- graph ready: `true`
- stale paths: `0`
- failed paths: `0`
- indexed files: `3306`
- symbols: `32050`
- edges: `141440`
- references: `64528`

Exact-token fallback remains separate:

- `rg -n "parse_task_selector" crates src tests`: `0.02s`, `21` hits

## Active Corpus Results

| Case | Expected primary | Current top owner | Time | Result |
| --- | --- | --- | ---: | --- |
| deploy provider export | `src/runner/deploy_command/provider_package.rs` | `src/runner/deploy_command/provider_package.rs` | `39.51s` | exact owner, posture acceptable, speed not acceptable |
| graph watch implementation | `crates/effigy-codegraph/src/watch.rs` | `crates/effigy-codegraph/src/watch.rs` | `20.20s` | exact owner, zero-reread posture, speed not acceptable |
| release orchestration | `crates/effigy-release/src/lib.rs` | `crates/effigy-cli/src/command_parsing_release.rs` | `33.32s` | acceptable alternate only; still not ideal |
| graph status stale detection | `crates/effigy-codegraph/src/index.rs` | `crates/effigy-codegraph/src/index.rs` | `106.86s` | exact owner, severe latency failure |
| task route parsing | `src/runner/execute/routing.rs` | `src/runner/execute/routing.rs` | `66.90s` | exact owner, severe latency failure |
| bundle source git | `crates/effigy-manifest/src/bundles/source.rs` | `crates/effigy-manifest/src/bundles/source.rs` | `137.84s` | exact owner, severe latency failure |
| graph agent docs | `docs/guides/076-code-graph-and-agent-workflows.md` | `docs/guides/076-code-graph-and-agent-workflows.md` | `6.83s` | exact owner, still slower than desired but materially better than code-heavy cases |

Active-corpus score:

- `6/7` exact expected primaries
- `1/7` acceptable alternate
- `0/7` hidden weak-owner failures
- `0/7` acceptable performance parity wins

## Baseline Comparison

Compared with the pinned `985` warm-index baseline:

- owner quality improved or held on every active non-exact-token case
- release architecture still does not promote the release library owner to the
  top slot
- warm-query time regressed sharply:
  - baseline range on active graph cases: roughly `1.88s` to `2.82s`
  - current fresh range: `6.83s` to `137.84s`

This means the lane succeeded on graph usefulness and coverage, but failed the
performance side of parity.

## Remaining Gaps

Not closed:

- warm `graph explore` query latency on larger live indexes
- fixture-backed parity execution for:
  - affected-test cases
  - cross-language cases
- stronger evidence for output byte cost versus direct file reads
- release-architecture ranking still preferring CLI parsing over the release
  library owner

Closed or accepted:

- exact-token work still belongs to `rg`
- source-body ranking evidence is indexed rather than broad per-query rereads
- traversal, route facts, affected workflow, docs, help, skill, and rustdoc
  all landed and are useful

## Interpretation

- Effigy graph is now a credible navigation tool for agents
- Effigy graph is **not** yet an honest "as good as or better than CodeGraph"
  story because the current warm-query cost can be worse than direct targeted
  file-system work on the live repo
- the correct next move is not more parity wording; it is a bounded latency and
  fixture-runner follow-up lane

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: the parity suite now has a real closeout decision backed by a fresh
  benchmark rerun, and the repo no longer needs to guess whether parity was
  achieved
- remains open: query-latency recovery, fixture-backed parity cases, and any
  later claim that Effigy equals or beats CodeGraph

## Next Task

No active ready card. Open a bounded follow-up planning lane for graph query
latency and fixture-backed parity execution before more graph parity work.
