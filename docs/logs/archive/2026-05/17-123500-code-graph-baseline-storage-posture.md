# Code Graph Baseline Storage Posture

Date: 2026-05-17
Roadmap: `g07.001`
Card: `901`

## Summary

Captured the first implementation baseline for Effigy's native code graph
surface.

This batch locks the initial ownership and dependency decisions so `902` can
implement storage and JSON contracts without reopening product scope.

## Decisions

### Ownership

- create a new first-party crate: `crates/effigy-codegraph`
- keep storage, indexing, extractor traits, and query APIs in that crate
- add CLI parsing in `crates/effigy-cli`
- add a thin runner command family under `src/runner/graph_command/`
- do not route graph through `effigy-builtin`

### Dependencies

- use `rusqlite` for graph storage
- require SQLite `bundled` and `fts5`
- use Rust `tree-sitter` bindings only
- defer grammar crates until their language cards land
- reuse `ignore` traversal posture already proven in `effigy-scan`
- defer parallel indexing and watch integration until the first storage/index
  passes are implemented

### Artifact And Contract Shape

- local graph artifact path: `.effigy/graph/graph.db`
- DB layout remains private
- public contract is CLI JSON with explicit `schema` and `version`
- initial normalized record families:
  - `file`
  - `symbol`
  - `edge`
  - `reference`
  - `diagnostic`
  - `index_run`
  - `extractor`

### Fixture Baseline

- Effigy repo itself for Rust, TOML, manifests, and docs
- existing manifest/include/task fixtures
- existing docs/contracts/guides surfaces
- later language cards add minimal PHP and JS/TS fixtures

## Evidence

- no existing `rusqlite` or `tree-sitter` dependency is present in the
  workspace yet
- `ignore::WalkBuilder` is already in production use at
  `crates/effigy-scan/src/support/traversal/walker.rs`
- top-level command families such as `docs`, `contracts`, `artifact`,
  `distribution`, and `release` already live under `src/runner/*_command`
- the workspace currently has no graph domain crate

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved in this report:
  - baseline product boundary for code graph work is now fixed
  - storage and command ownership decisions are recorded
  - dependency posture is fixed for `902`
- remains open:
  - graph storage implementation
  - graph JSON contract types
  - index/status commands
  - first-party extractors
  - query/context surfaces

## Validation

Commands used:

```bash
rg -n "rusqlite|sqlite|tree-sitter|ignore::|walkdir|fts5|notify" Cargo.toml crates src
cargo metadata --format-version 1 --no-deps
find crates -maxdepth 2 -name Cargo.toml | sort
rg -n "WalkBuilder|ignore::WalkBuilder|struct .*Command|enum .*Command|graph" crates src -g '*.rs'
```

## Next Task

Execute `902`.
