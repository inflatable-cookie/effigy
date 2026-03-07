# Translation Memos

Effigy-facing recommendations derived from research.

## Purpose

Translation memos bridge research findings and Effigy action. They:
1. Summarize external evidence
2. Make specific recommendations for Effigy
3. Define what must be true before adoption
4. Propose promotion targets (concept work, roadmap, watch, reject)

## Status

| Memo | Status | Track | Recommendation |
|------|--------|-------|----------------|
| 001-toml-configuration-validation.md | Complete | Track 01 | Validate TOML as correct choice |
| 002-caching-strategy.md | Draft | Track 02 | Content-addressable cache with local+remote tiers |
| 003-file-watching-strategy.md | Draft | Track 03 | Use watchexec crate for cross-platform watching |
| 004-dag-execution-strategy.md | Draft | Track 04 | Keep current DAG model, add cycle detection |
| 005-tui-patterns.md | Draft | Track 05 | Retain multi-pane TUI with refinements |
| 006-shell-completion-strategy.md | Draft | Track 06 | Hybrid: clap_complete + dynamic tasks |
| 007-error-reporting-strategy.md | Draft | Track 07 | Rustc-inspired format with error codes |
| 008-workspace-strategy.md | Draft | Track 08 | Distributed catalogs + change detection |
| 009-cross-platform-strategy.md | Draft | Track 09 | Native shell + platform conditionals |
| 010-environment-strategy.md | Draft | Track 10 | TOML env + .env loading + secret providers |
| 011-remote-execution-strategy.md | Draft | Track 11 | S3-compatible remote cache, scale later |

## Memo Lifecycle

1. **Draft**: Written after value track synthesis
2. **Review**: Checked against evidence quality rules
3. **Promotion decision**: 
   - `concept work` → docs/concepts/
   - `roadmap execution` → docs/roadmaps/
   - `watch` → stay here, monitor
   - `reject` → documented decision not to pursue

## Research Program Status

**Phase 1 ✅ COMPLETE** — 5 memos (001-005)
**Phase 2 ✅ COMPLETE** — 5 memos (006-010)

**TOTAL: 10 translation memos**

**Next:**
- Option 1: Phase 3 — 5 more memos (011-015)
- Option 2: Begin implementation — Promote memos to roadmaps

