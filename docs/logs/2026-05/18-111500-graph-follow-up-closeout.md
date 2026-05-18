# Graph Follow-Up Closeout

Date: 2026-05-18  
Roadmap: [`g07.013`](../roadmaps/g07/013-graph-follow-up-performance-and-fixture-reliability.md)  
Batch card: [`935`](../roadmaps/g07/batch-cards/935-close-graph-follow-up-proof.md)  
Strict lane: [`086`](../specs/086-graph-follow-up-performance-and-fixture-reliability-strict-lane.md)

## What Changed

- closed the first post-launch graph hardening tranche
- landed a real file-level incremental/no-op index path
- reduced query-time graph materialization costs in `status`, `search`, and
  `context`
- removed the known seven-path full-repo graph failure set without hiding or
  suppressing those paths
- left semantic compose issues on template-heavy bundle/export files as warning
  diagnostics rather than extractor failures

## Measured Delta

Compared with the locked `g07.012` closeout baseline:

- no-op full-repo `graph index --json`
  - baseline: `148.39s`
  - after `g07.013`: `17.71s`
  - improvement: `88.1%`
- `graph status --json`
  - baseline: `2.34s`
  - after `g07.013`: `0.48s`
  - improvement: `79.5%`
- `graph search release --limit 5 --json`
  - baseline: `4.43s`
  - after `g07.013`: `0.29s`
  - improvement: `93.5%`
- `graph context "trace release orchestrator" --language rust --max-files 6 --max-bytes 2048 --json`
  - baseline: `2.73s`
  - after `g07.013`: `1.58s`
  - improvement: `42.1%`
- full-repo failed graph paths
  - baseline: `7`
  - after `g07.013`: `0`

## Final State

- graph DB size: `155,811,840` bytes (`148.6 MiB`)
- file records: `3208`
- symbols: `31153`
- edges: `139109`
- references: `63051`
- diagnostics: `6` warnings, `0` errors
- failed paths: `0`
- direct `rg -n "release orchestrator" src crates docs`: `0.04s`

## Remaining Limits

- no-op indexing is now cheap, but still pays for a full repo file walk
- lexical `rg` remains faster for tiny raw text lookups than graph search
- template-heavy bundle/export manifests still surface warning-level semantic
  compose diagnostics because the manifest composer expects exact TOML

## Validation

- `cargo test -p effigy-codegraph`
- `cargo test graph -- --nocapture`
- `cargo build --bin effigy`
- `./target/debug/effigy graph index --json`
- `./target/debug/effigy graph status --json`
- `./target/debug/effigy graph search release --limit 5 --json`
- `./target/debug/effigy graph context "trace release orchestrator" --language rust --max-files 6 --max-bytes 2048 --json`
- `rg -n "release orchestrator" src crates docs`
- `./target/debug/effigy docs check paths ...`
- `git diff --check`

## Vision Target Delta

- primary vision tags touched: `CONTRACT`, `OPERATE`, `MAINT`
- moved: graph follow-up hardening limits from `g07.012` -> cheap no-op
  indexing, faster graph queries, and full-repo `failed_paths = []`
- remains open: no active `g07` card; next work is optional and should start
  from incremental scan cost, lexical-search competitiveness, or warning-level
  template-manifest composition depth
