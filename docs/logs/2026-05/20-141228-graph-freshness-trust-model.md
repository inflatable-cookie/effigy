# Graph Freshness Trust Model

Date: 2026-05-20
Card: [`1023`](../../roadmaps/g07/batch-cards/1023-tighten-graph-freshness-trust-model.md)
Strict lane: [`096`](../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

## Summary

Added a compact freshness trust contract to graph status and query payloads.

Agents no longer need to infer trust from `ready` plus large stale-path lists.
The graph now reports one compact state, one summary sentence, whether the
current index is usable, and the stale/failed path counts while preserving the
full path diagnostics for follow-up work.

## Vision Target Delta

Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`

Baseline:

- freshness existed, but trust still had to be inferred from a mix of
  `ready`, `index_present`, and large stale-path arrays
- missing-index repos did not expose a compact first-glance state
- text-mode graph output did not call trust out directly

Current:

- graph status JSON now includes `freshness.state`, `summary`, `usable`,
  `stale_path_count`, and `failed_path_count`
- graph query JSON surfaces reuse the same compact trust payload
- text-mode status and query output now print the same trust state and summary
- missing-index, stale, degraded, and ready states are explicit across repos

Remaining:

- behavioral-query ranking still needs work in `1024`
- edit-target/test-target packet sharpness still needs work in `1025`

## Implementation Notes

Changed surfaces:

- `crates/effigy-codegraph/src/json.rs`
- `crates/effigy-codegraph/src/index.rs`
- `crates/effigy-codegraph/src/query/mod.rs`
- `src/runner/graph_command.rs`

Contract shape:

- `missing-index`
  - no usable local graph index
- `refresh-recommended`
  - index is usable, but stale paths exist
- `degraded`
  - index is current enough to use, but failed paths make results incomplete
- `ready`
  - index is current and has no failed paths

One implementation nuance mattered during proof:

- `index_present` is not a trustworthy proxy for usability on its own because
  opening the graph store can materialize the local DB path before it contains
  a usable indexed corpus
- the stable trust contract is therefore `freshness.state` plus `usable`, not
  `index_present` alone

## Proof

Focused tests:

- `cargo test -p effigy-codegraph --quiet`
- `cargo test -p effigy graph_ -- --nocapture`

Live checks:

- Effigy repo:
  - `freshness.state=refresh-recommended`
  - `freshness.usable=true`
- Underlay repo:
  - `freshness.state=missing-index`
  - `freshness.usable=false`
- decodelabs app:
  - `freshness.state=missing-index`
  - `freshness.usable=false`
- decodelabs library:
  - `freshness.state=missing-index`
  - `freshness.usable=false`

Interpretation:

- the trust contract now works for both indexed and non-indexed repos
- the change is generic and does not depend on Effigy-specific vocabulary or
  paths

## Next

Move to `1024`: improve behavioral query ranking and vocabulary.
