# Graph Explore Baseline

Date: 2026-05-18

## Summary

Captured the baseline for `g07.031` before implementing
`effigy graph explore`.

Current `graph context` is useful for owner discovery, but it does not yet give
agents enough assembled context to avoid immediate follow-up file reads. It also
has no native result limit flag, so callers must trim JSON themselves.

## Baseline Contract Target

`effigy graph explore "<question>" --json` should return:

- `query`: original and normalized query text
- `index`: graph path, freshness state, indexed file count, and revision when
  available
- `summary`: deterministic overview assembled from selected graph facts
- `primary`: ranked owner files and symbols
- `excerpts`: bounded source/doc excerpts with path, range, language, role,
  score, reason, and text
- `relations`: nearby callers, callees, tests, docs, config, and roadmap files
  when known
- `overflow`: files omitted because of ranking or size limits
- `guidance`: freshness and exact-match fallback notes

The text renderer should expose the same facts in a compact operator-readable
shape. JSON remains the agent contract.

## Benchmark Results

| Query | `graph context` top result | Time | Baseline read posture |
| --- | --- | ---: | --- |
| `trace deploy provider export` | `src/runner/deploy_command/provider_context.rs` | 1.94s | likely 2-4 file reads; external provider package files appear in top six |
| `trace graph watch implementation` | `crates/effigy-codegraph/src/watch.rs` | 1.58s | likely 2-3 file reads; good owner set |
| `understand release orchestration` | `crates/effigy-release/Cargo.toml` | 1.87s | likely 3-5 file reads; implementation starts at rank 3-6 |
| `find graph status stale detection` | `docs/logs/2026-05/18-131500-tighten-stale-and-status-scan-cost.md` | 1.88s | weak owner result; likely broad follow-up search |
| `docs for graph agent workflow` | `docs/guides/076-code-graph-and-agent-workflows.md` | 1.55s | good docs result; likely 1 file read |
| `where are task routes parsed` | `src/runner/tasks_command/prepare.rs` | 1.55s | partial result; likely follow-up search for parser/routing ownership |
| `what changes when a bundle source is git` | `docs/contracts/020-remote-bundle-sources-git-and-oci-delivery-contract.md` | 1.58s | docs-heavy result; likely follow-up implementation search |

Direct broad `rg` is still much faster but too noisy for architecture
questions:

| Search | Time | Hits |
| --- | ---: | ---: |
| `deploy|provider|export` | 0.05s | 3726 |
| `graph|watch|implementation` | 0.05s | 3476 |
| `release|orchestration` | 0.08s | 7281 |
| `bundle|source|git` | 0.08s | 7574 |
| `graph status|stale` in graph/doc slices | 0.01s | 292 |

## Implementation Targets For `982`

- add native result bounds for `explore`
- assemble excerpts from the top ranked files instead of only naming files
- prefer implementation excerpts for implementation wording even when docs or
  logs have many token hits
- include docs/contracts as related context when they explain behavior but do
  not own implementation
- include enough provenance that agents can cite or verify without opening the
  same files immediately
- keep exact `rg` as the fallback for token-level verification

## Vision Target Delta

- tags: `OPERATE`, `MAINT`, `CONTRACT`
- baseline: `graph context` narrows many searches, but still leaves agents
  reading files to assemble the working map
- current: contract and measured baseline are ready for implementation
- remains open: implement and validate `graph explore`

## Next Task

Execute `982`.
