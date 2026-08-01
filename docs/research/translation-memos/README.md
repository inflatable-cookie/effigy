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
| 012-ci-cd-integration.md | Draft | Track 12 | Keep execution CI-agnostic |
| 013-ide-integration.md | Draft | Track 13 | Prefer standard machine-readable interfaces |
| 014-plugin-architecture.md | Draft | Track 14 | Stable, function-based extension surface |
| 015-telemetry-and-observability.md | Draft | Track 15 | Transparent, consent-based telemetry |
| 016-secure-secrets-management.md | Draft | Track 16 | Offline-first secure secret providers |
| 016b-external-provider-secrets.md | Draft | Track 16 | Reference external provider secrets |
| 016c-varlock-integration.md | Draft | Track 16 | Integrate Varlock as an external dependency |
| 017-apple-containers-runtime-backend.md | Complete | Runtime | Watch-only native prototype; boot-time discovery blocks support |

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

**TOTAL: 19 translation memos**

**Next:** Promote complete memos only when their owner and evidence plan are
explicit. Reassess Apple Containers when boot-time service discovery or an
equivalent safe repair becomes available.
