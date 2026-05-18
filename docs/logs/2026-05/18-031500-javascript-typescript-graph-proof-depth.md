# JavaScript Typescript Graph Proof Depth

Date: 2026-05-18
Roadmap: `g07.009`
Batch card: `909`

## What Changed

- rewrote the JS/TS extractor to emit stronger graph facts for:
  - deterministic relative import resolution
  - unresolved package imports
  - export and default-export edges
  - functions, classes, methods, interfaces, enums, type aliases
  - variable-backed component-like declarations
  - call-site references
- added parse-error diagnostics that keep broken JS/TS files indexed
- added conservative React-module classification for JSX/TSX files

## Current State

- `909` is complete
- next ready card is `911`
- JS/TS graph facts now cover real frontend module navigation instead of only
  shallow declarations and raw imports

## Validation

- `cargo test -p effigy-codegraph`
- `cargo check -p effigy-cli -p effigy`
- `cargo test graph -- --nocapture`
- `cargo fmt --all -- --check`

## Vision Target Delta

- tags: `ROUTE`, `CONTRACT`, `MAINT`
- moved:
  - JS/TS graph depth: shallow declarations/unresolved imports only -> resolved relative imports, exports, component-like symbols, parse diagnostics
  - failure posture: broken file may disappear into parser recovery -> warning diagnostics with retained indexing
  - frontend navigation: raw file/module facts only -> useful import/export/component ownership graph
- remains open:
  - `911` bounded context-pack ranking proof
  - `912` performance closeout
