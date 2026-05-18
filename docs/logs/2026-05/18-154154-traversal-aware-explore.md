# Traversal-Aware Explore

Date: 2026-05-18  
Roadmap: [`g07.038`](../../roadmaps/g07/038-traversal-aware-explore-assembly.md)  
Batch card: [`987`](../../roadmaps/g07/batch-cards/987-implement-traversal-aware-explore.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- taught `graph explore` to walk a bounded one-hop neighborhood around primary
  owners instead of only echoing independently ranked files
- added traversal neighbors for:
  - resolved symbol edges
  - resolved file edges such as JS/TS `import-file`
  - doc/file support edges
  - bounded unresolved Rust and JS call/import targets when symbol or file
    matches are strong enough
- filtered structural `contains` noise and suppressed same-file traversal
  neighbors so relation slots are spent on secondary files
- appended related-file excerpts from traversal neighbors within the existing
  byte budget
- added regression coverage for:
  - JS/TS import/call traversal into a related helper file
  - unresolved Rust call traversal into a helper file

## Validation

- `cargo test -p effigy-codegraph`
- `cargo fmt --all -- --check`
- `cargo clippy -p effigy-codegraph -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `target/debug/effigy graph index --json`
- `target/debug/effigy graph status --json`

New regressions:

- `graph_explore_traverses_import_neighbors_and_emits_related_file_excerpts`
- `graph_explore_traverses_unresolved_rust_call_neighbors`

## Warm Corpus Snapshot

Warm index after reindex:

- ready: `true`
- stale paths: `0`
- indexed files: `3298`
- symbols: `31913`
- edges: `141379`
- references: `63872`

Live repo spot checks after the traversal batch:

| Case | Query | Current top owner | Time | Current posture |
| --- | --- | --- | ---: | --- |
| ownership | `trace deploy provider export` | `src/runner/deploy_command/provider_package.rs` | `1.65s` | owner still correct; related provider files appear in the packet |
| call-flow | `trace graph watch implementation` | `crates/effigy-codegraph/src/watch.rs` | `1.65s` | owner still correct; watch packet stays bounded and excerpt-first |
| architecture | `understand release orchestration` | `crates/effigy-cli/src/command_parsing_release.rs` | `1.65s` | release library still appears in the packet, but architecture ranking is not yet fully solved |

Fixture-backed traversal proofs now cover the cases where the stored graph has
the right edges but `explore` previously failed to expose them:

- JS/TS helper flows now add `web/util.ts` as a related neighbor and excerpt
- Rust unresolved call flows now add `src/helper.rs` as a related neighbor and
  excerpt

## Interpretation

- `explore` now has a real traversal stage instead of only returning ranked
  owners and symbol echoes
- the bounded traversal is useful today on resolved imports/docs and on a
  practical subset of unresolved Rust/JS calls
- the live architecture query is still not where it needs to be for parity:
  extractor coverage and framework/entrypoint facts remain the next leverage
  point, not more ad hoc ranking tweaks inside `explore`

## Residual Limits

- traversal is still one hop
- unresolved matching is heuristic and bounded, not compiler-grade resolution
- many real repos still will not emit enough high-value edges until extractor
  coverage expands in `988` and route/entrypoint facts land in `989`
- `explore` still carries duplicate same-path excerpts from ranked symbol items
  in some live queries; that polish belongs in the adoption and packet-hardening
  cards, not here

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: `graph explore` now traverses bounded topology and can explain some
  secondary files through explicit graph relations instead of only ranked owner
  heuristics
- remains open: wider extractor coverage, framework/route facts, no-reread
  source packets, affected-test workflow, and final parity proof

## Next Task

Execute `988`.
