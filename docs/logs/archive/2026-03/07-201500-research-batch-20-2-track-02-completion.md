# Research Batch 20.2: Track 02 Completion

Date: 2026-03-07
Roadmap: g01.020
Batch: 20.2

## Summary

Completed Batch 20.2 of Research Phase 1 (Core Execution). Three tool dossiers and Track 02 value track synthesis completed.

## Deliverables

### Tool Dossiers (3)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [Bazel](../../../research/tool-dossiers/bazel.md) | Complete | Content-addressable caching is correct but complex; hermeticity is expensive |
| [Turbo](../../../research/tool-dossiers/turbo.md) | Complete | Pattern-based hashing balances speed and correctness; Rust rewrite validates Effigy's choice |
| [sccache](../../../research/tool-dossiers/sccache.md) | Complete | Wrapper pattern enables easy adoption; cloud storage integration is straightforward |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 02: Caching Strategies](../../../research/value-tracks/02-caching-strategies.md) | Complete | Content-addressable with configurable granularity; local + remote tiers |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [002: Caching Strategy](../../../research/translation-memos/002-caching-strategy.md) | Draft | Prototype before promotion |

## Key Findings

### Caching Strategy Validated

Comparative analysis recommends content-addressable caching:

| Approach | Tool | Verdict |
|----------|------|---------|
| Timestamps | Make | ❌ Clock skew, false negatives |
| Full content hashing | Bazel | ✅ Correct but complex |
| Pattern-based hashing | Turbo | ✅ Good balance for Effigy |
| Compiler wrapper | sccache | ⚠️ Limited scope |

### Proposed Effigy Cache Design

```toml
[cache]
enabled = true

[cache.local]
enabled = true
dir = ".effigy/cache"
max_size = "1GB"

[cache.remote]
enabled = false
backend = "s3"  # s3 | gcs | azure
bucket = "my-team-cache"
```

Per-task configuration:
```toml
[tasks.build.cache]
enabled = true
inputs = ["src/**", "Cargo.*"]
outputs = ["target/release/app"]
env = ["RUSTFLAGS"]
```

### Patterns to Adopt

- **Two-tier caching**: Local fast + remote shared
- **Configurable inputs**: Users declare what affects output
- **HTTP-based protocol**: Simple, sccache-compatible
- **Statistics visibility**: Show hit rates in doctor/tasks

### Patterns to Reject

- Timestamps alone (Make): Unreliable
- Full hermeticity (Bazel): Complexity overkill
- Wrapper-only approach (sccache): Too limited

## Evidence Quality

| Source Type | Count | Confidence |
|-------------|-------|------------|
| Official documentation | 9 | high |
| GitHub repos | 3 | high |
| Blog posts/announcements | 3 | high |
| Community issues | 2 | medium |

## Next Batch

**Batch 20.3**: Track 03 — Watch Mode and File Monitoring

Tools to study:
- cargo-watch (Rust ecosystem)
- watchexec (general-purpose)
- entr (Unix philosophy)

## Acceptance Criteria

- [x] 3 dossiers complete with source inventories
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] Cache design proposed with configuration examples

## Outcome

Batch 20.2 complete. Content-addressable caching validated as the correct approach. Proposed design balances correctness (content hashing) with usability (configurable granularity). Memo 002 remains in "draft" state pending prototype validation.

Ready to proceed to Batch 20.3 (Watch Mode).

