# Research Batch 20.3: Track 03 Completion

Date: 2026-03-07
Roadmap: g01.020
Batch: 20.3

## Summary

Completed Batch 20.3 of Research Phase 1 (Core Execution). Three tool dossiers and Track 03 value track synthesis completed.

## Deliverables

### Tool Dossiers (3)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [cargo-watch](../../research/tool-dossiers/cargo-watch.md) | Complete | Debouncing essential; smart defaults (ignore target/) reduce noise |
| [watchexec](../../research/tool-dossiers/watchexec.md) | Complete | Cross-platform library ideal for reuse; handles platform quirks |
| [entr](../../research/tool-dossiers/entr.md) | Complete | Minimalism has limits; no debouncing causes UX pain |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 03: Watch Mode](../../research/value-tracks/03-watch-mode-and-file-monitoring.md) | Complete | Use watchexec crate; debounce 500ms; smart defaults |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [003: File Watching Strategy](../../research/translation-memos/003-file-watching-strategy.md) | Draft | Prototype watchexec integration |

## Key Findings

### File Watching Strategy Validated

| Approach | Tool | Verdict |
|----------|------|---------|
| Minimal (no debouncing) | entr | ❌ Poor UX for rapid changes |
| Balanced (debounce + defaults) | cargo-watch | ✅ Good UX |
| Library (flexible) | watchexec | ✅ Right abstraction for Effigy |

### watchexec Recommended for Effigy

```rust
// Proposed integration
use watchexec::Watchexec;
use watchexec::config::Config;

let config = Config::default()
    .path(".")
    .filter(|path| !path.starts_with("target/"));

let wx = Watchexec::new(config, |action| {
    // Debounce and run Effigy task
})?;
```

### Configuration Design

```toml
[watch]
debounce = 500  # milliseconds
ignore = [".git/", ".effigy/", "target/"]

[tasks.dev.watch]
enabled = true
paths = ["src/**", "Cargo.toml"]
```

### Patterns to Adopt

- **Debouncing**: 500ms default to batch rapid changes
- **Smart ignores**: target/, .git/, .effigy/ automatically ignored
- **Cross-platform library**: watchexec handles FSEvents/inotify/Windows
- **Clear feedback**: Show what triggered the run

### Patterns to Reject

- No debouncing (entr): Causes frustration
- Polling as primary: Too resource-intensive
- Custom implementation: Must handle platform quirks
- Tool-specific watching (cargo-watch): Effigy is language-agnostic

## Evidence Quality

| Source Type | Count | Confidence |
|-------------|-------|------------|
| Official documentation | 6 | high |
| Source code | 3 | high |
| Library API docs | 2 | high |
| Community usage | 3 | high |

## Next Batch

**Batch 20.4**: Track 04 — DAG Execution and Scheduling

Tools to study:
- Bazel (Skyframe evaluation)
- Dagger (container-based DAG)

## Acceptance Criteria

- [x] 3 dossiers complete with source inventories
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] watchexec integration proposed with code example

## Outcome

Batch 20.3 complete. watchexec crate validated as the right approach for Effigy's watch mode. The library provides proven cross-platform file watching without the complexity of custom implementation.

Ready to proceed to Batch 20.4 (DAG Execution).

