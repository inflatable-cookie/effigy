# Failed Graph Fixture Path Indexing

Date: 2026-05-18  
Roadmap: [`g07.016`](../roadmaps/g07/016-failed-graph-fixture-path-reliability.md)  
Batch card: [`934`](../roadmaps/g07/batch-cards/934-fix-failed-graph-fixture-path-indexing.md)  
Strict lane: [`086`](../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

## What Changed

- made the manifest graph extractor template-aware instead of treating
  Jinja-style bundle/export files as hard TOML parse failures
- added a sanitized TOML fallback for template-bearing files and a final lossy
  structural section scanner when sanitized TOML still cannot parse cleanly
- extended template sanitization to cover template expressions inside quoted
  TOML strings, including embedded quotes
- skipped blank unresolved manifest targets instead of emitting invalid graph
  edges for placeholder values like empty provider ids
- bumped the manifest extractor version to force reindex of existing TOML graph
  records

## Measured Delta

Compared with the locked `g07.012` closeout baseline:

- full-repo `graph index --json` failed paths
  - baseline: `7`
  - after `934`: `0`
- full-repo diagnostics
  - baseline hard failures: `7`
  - after `934`: `0` errors, `6` warnings
- full-repo follow-up index after manifest reindex
  - duration: `30.68s`
  - result: `3207` indexed files, `31145` symbols, `139096` edges,
    `63051` references

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo build --bin effigy`
- full-repo `./target/debug/effigy graph index --json`
- `./target/debug/effigy docs check paths ...`
- `git diff --check`

## Remaining Limits

- template-heavy bundle/export manifests still fall back to warning-level
  semantic compose diagnostics because the real manifest composer expects exact
  TOML, not templated source
- `935` still needs to refresh the lane-wide before/after proof against the
  full `g07.012` baseline

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: full-repo graph failed-path set `7 -> 0` by turning template-heavy
  manifest-like files into structural graph inputs instead of extractor
  failures
- remains open: `935`
