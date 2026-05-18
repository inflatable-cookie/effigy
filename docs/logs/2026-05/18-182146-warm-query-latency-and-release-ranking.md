# Warm Query Latency And Release Ranking

Date: 2026-05-18  
Roadmap: [`g07.047`](../../roadmaps/g07/047-warm-query-latency-and-release-ranking.md)  
Batch card: [`997`](../../roadmaps/g07/batch-cards/997-reduce-warm-query-latency-and-fix-release-ranking.md)  
Strict lane: [`092`](../../specs/092-codegraph-parity-follow-up-strict-lane.md)

## What Changed

- removed duplicated graph query work inside `explore`
  - reused one loaded graph snapshot across `context` and `explore`
  - stopped calling full `status()` from `explore`
  - reused one freshness calculation and one counts read
- replaced broad per-query edge scanning in traversal with adjacency maps
- replaced repeated unresolved-neighbor whole-symbol scans with indexed token
  candidate lookup plus cached expansion
- restored unresolved-call file projection so helper-owner excerpts still land
- added a narrow crate-root architecture bonus so broad release-orchestration
  questions prefer the release library owner over CLI parsing glue when the
  rest of the evidence is tied

## Measured Delta

Fresh-binary live-repo timings after the `g07.045` closeout baseline:

| Query | `g07.045` closeout | current | top owner |
| --- | ---: | ---: | --- |
| `trace deploy provider export` | `39.51s` | `5.64s` | `src/runner/deploy_command/provider_package.rs` |
| `trace graph watch implementation` | `20.20s` | `4.17s` | `crates/effigy-codegraph/src/watch.rs` |
| `understand release orchestration` | `33.32s` | `3.86s` | `crates/effigy-release/src/lib.rs` |
| `find graph status stale detection` | `106.86s` | `7.56s` | `crates/effigy-codegraph/src/index.rs` |
| `where are task routes parsed` | `66.90s` | `7.79s` | `src/runner/execute/routing.rs` |
| `what changes when a bundle source is git` | `137.84s` | `8.16s` | `crates/effigy-manifest/src/bundles/source.rs` |
| `docs for graph agent workflow` | `6.83s` | `4.40s` | `docs/guides/076-code-graph-and-agent-workflows.md` |

## Interpretation

- the live warm-query regression was real and severe
- the main cost was not ranking itself; it was redundant graph loading,
  redundant freshness/status work, whole-graph edge scans, and repeated
  unresolved-neighbor expansion
- after the cut, the active live-repo corpus is back in a credible warm-query
  range for agent use
- the remaining release-orchestration ranking miss is resolved for the active
  benchmark query

## Validation

- `cargo test -p effigy-codegraph`
- `cargo clippy -p effigy-codegraph -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- targeted traversal regressions:
  - `graph_explore_traverses_unresolved_rust_call_neighbors`
  - `graph_explore_traverses_import_neighbors_and_emits_related_file_excerpts`

## Residual Limits

- this slice fixes live-repo warm-query behavior, not the deferred fixture-runner
  proof cases
- parity still cannot fully close until the affected-test and cross-language
  deferred cases are executable

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: warm live-repo `graph explore` queries dropped from tens or hundreds
  of seconds into single-digit seconds while preserving active-corpus owner
  quality
- remains open: fixture-backed parity execution and final follow-up closeout

## Next Task

Execute `998`.
