# Manifest And Markdown Graph Depth

Date: 2026-05-18
Roadmaps: `g07.006`, `g07.007`
Batch cards: `906`, `907`

## What Changed

- deepened the manifest graph indexer so Effigy-specific config emits richer
  ownership facts for:
  - docs policy indexes and next actions
  - test runners and suites
  - secrets declarations without reading values
  - deploy providers, provider sources, and deploy target/provider links
  - state stacks, layers, captures, and inline hook/task run shapes
- widened TOML coverage so included manifest fragments are indexed instead of
  sitting in skipped/stale state
- downgraded manifest semantic compose failures from fatal file-index failures
  to warning diagnostics with structural fallback
- deepened the Markdown indexer so docs now emit:
  - code fence symbols
  - code fence language metadata
  - resolved local file references from text/code spans
  - resolved local Markdown link-to-file edges

## Current State

- `906` is complete
- `907` is complete
- next ready card is `908`
- manifest graph facts now cover the core Effigy config surfaces instead of
  stopping at tasks/containers/bundles only
- docs graph facts now distinguish headings, local links, code fences, and
  direct file references

## Validation

- `cargo test -p effigy-codegraph`
- `cargo check -p effigy-cli -p effigy`
- `cargo test graph -- --nocapture`
- `cargo fmt --all -- --check`

## Vision Target Delta

- tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved:
  - manifest graph depth: task/container spine only -> docs/test/secrets/deploy/state facts
  - Markdown graph depth: headings/raw links only -> headings/links/code fences/path refs
  - manifest robustness: semantic compose failure aborts file index -> warning diagnostic with structural fallback
- remains open:
  - `908` PHP extractor proof depth
  - `909` JavaScript/TypeScript extractor proof depth
  - `911` bounded context-pack ranking proof
  - `912` performance closeout
