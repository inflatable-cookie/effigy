# Graph Explore Implementation Closeout

Date: 2026-05-18

## Summary

Implemented `effigy graph explore` and closed the `g07.030` graph explore lane.

The command reuses the existing graph context ranker, then assembles a richer
agent packet with primary owners, excerpts, related symbols, index freshness,
overflow, and guidance. It is intentionally a navigation tool, not an exact
search replacement.

## What Changed

- added `GraphSubcommand::Explore`
- added `effigy graph explore <REQUEST>` parsing with `--max-files`,
  `--max-bytes`, `--language`, `--path`, `--repo`, and `--json`
- added `effigy.graph.explore.v1`
- added graph explore payload types:
  - `query`
  - `index`
  - `summary`
  - `primary`
  - `excerpts`
  - `relations`
  - `overflow`
  - `guidance`
- added text rendering
- added parser, JSON, query, runner, text, and help tests
- updated graph guide docs, command reference, rustdoc, changelog, and bundled
  Effigy skill

## Benchmark Closeout

Compared with the baseline `graph context -> read files -> rg` workflow:

| Query | `graph explore` top owner | Time | Result |
| --- | --- | ---: | --- |
| `trace deploy provider export` | `src/runner/deploy_command/provider_context.rs` | 1.99s | useful packet; still likely needs provider package follow-up |
| `trace graph watch implementation` | `crates/effigy-codegraph/src/watch.rs` | 1.98s | useful packet; first excerpts cover the owner files |
| `understand release orchestration` | `crates/effigy-release/Cargo.toml` | 1.97s | ranking still imperfect; excerpts reduce reads but top owner is not ideal |
| `find graph status stale detection` | `docs/logs/archive/2026-05/18-131500-tighten-stale-and-status-scan-cost.md` | 2.14s | weak top owner; direct verification still needed |
| `docs for graph agent workflow` | `docs/guides/076-code-graph-and-agent-workflows.md` | 4.00s | good docs packet |
| `where are task routes parsed` | `src/runner/tasks_command/prepare.rs` | 2.71s | partial owner packet |
| `what changes when a bundle source is git` | `docs/contracts/020-remote-bundle-sources-git-and-oci-delivery-contract.md` | 4.00s | useful docs context; implementation follow-up still likely |

Interpretation:

- `explore` improves first-call payload quality by returning excerpts and
  guidance, not by making graph queries faster.
- It should reduce immediate file opens for strong owner queries such as graph
  watch and deploy provider export.
- It does not yet fix all ranking weaknesses. Some broad queries still surface
  docs/logs/contracts before implementation.
- No CodeGraph-style percentage claim is justified yet.

## Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy-codegraph`
- `cargo test graph_ --lib`
- `cargo test parse_graph --lib`
- `effigy docs check paths ...`
- `effigy docs check links ...`

## Vision Target Delta

- tags: `OPERATE`, `MAINT`, `CONTRACT`
- baseline: graph context narrowed file sets but did not assemble enough
  first-pass context for agents
- current: graph explore provides a one-call agent packet with excerpts,
  relations, freshness, overflow, and guidance
- remains open: ranking quality for broad implementation questions can improve
  further, but no active card remains

## Next Task

No active `g07` task remains. A later tranche can target explore-specific
ranking if day-to-day usage shows it is worth the extra tuning.
