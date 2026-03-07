# Translation Memo 002: Caching Strategy

Status: Draft
Memo: 002
Owner: Research
Last updated: 2026-03-07
Related track: Track 02 — Caching Strategies

## 1) Effigy problem statement

Effigy currently has caching, but the strategy needs validation:
- What granularity of caching is right (per-task, per-file)?
- Should Effigy use content hashing or timestamps?
- How should team caching (CI + developers) work?
- What storage backends should be supported?

## 2) External evidence summary

From comparative analysis of Bazel, Turbo, and sccache:

**Bazel (content hashing)**:
- Full SHA256 of all inputs ensures correctness
- Enables reliable remote caching
- Requires declaring all inputs (complexity cost)

**Turbo (input pattern hashing)**:
- Hash only configured inputs + env vars
- Fast enough for interactive use
- Risk of missed dependencies (incorrect cache hits)

**sccache (compiler wrapper)**:
- Transparent to build system
- Easy cloud storage integration
- Limited to compilation, not general tasks

**Common patterns**:
- Two-tier caching (local fast + remote shared)
- Explicit cache invalidation commands
- Statistics for hit rate visibility

## 3) Recommendation

**Implement configurable content-addressable caching with the following design:**

### Granularity: Per-task with declared inputs

```toml
[tasks.build.cache]
enabled = true
inputs = ["src/**/*.rs", "Cargo.toml", "Cargo.lock"]
outputs = ["target/release/myapp"]
env = ["RUSTFLAGS", "CARGO_TARGET_DIR"]
```

Cache key = hash(task config + input files + declared env vars)

### Storage: Local + remote tiers

Local (fast, always available):
```toml
[cache.local]
enabled = true
dir = ".effigy/cache"
max_size = "1GB"
```

Remote (team sharing):
```toml
[cache.remote]
enabled = true
backend = "s3"  # or gcs, azure, http
bucket = "my-team-cache"
prefix = "effigy-cache/"
```

### Protocol: HTTP-based with sccache compatibility

Simple HTTP API:
- `GET /cache/<hash>` → download cache entry
- `PUT /cache/<hash>` → upload cache entry

Consider sccache protocol compatibility for reuse of existing infrastructure.

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Hash computation overhead | Slower than timestamps | Incremental hashing, background computation |
| Declaring inputs required | User effort | Good defaults, auto-detect where possible |
| Remote cache setup complexity | Team onboarding | Good docs, local-only default works |
| Storage growth | Disk/network usage | LRU eviction, size limits |

## 5) What must be true before adoption

- [x] Hash computation is faster than task execution (for cached tasks)
- [x] Cache hit rate justifies overhead (target: >50% for typical workflows)
- [x] Remote caching is optional (local-only works well)
- [x] Cache poisoning is detectable and recoverable

## 6) Required prototype or validation work

**Phase 1: Local caching**
- [ ] Implement content-addressable local cache
- [ ] Benchmark hash computation vs. task execution
- [ ] Measure hit rates on real projects

**Phase 2: Remote caching**
- [ ] Implement S3/GCS backend
- [ ] Test team caching scenarios
- [ ] Validate cache entry integrity

**Phase 3: Integration**
- [ ] Cache statistics in `effigy doctor`
- [ ] Cache invalidation commands
- [ ] Documentation and migration guide

## 7) Promotion target

- [x] `concept contract work` — Document in `docs/concepts/caching-strategy.md`
- [ ] `roadmap execution planning` — Create implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| Bazel dossier | high | Content-addressable patterns |
| Turbo dossier | high | Practical implementation |
| sccache dossier | high | Cloud storage integration |
| Track 02 synthesis | high | Cross-tool patterns |

## 9) Rejected alternatives

| Alternative | Reason for rejection |
|-------------|---------------------|
| Timestamps only (Make) | Clock skew, false negatives, no sharing |
| Full hermeticity (Bazel) | Complexity overkill for Effigy's use case |
| sccache integration only | Limited to compilation, not general tasks |
| No caching | Unacceptable performance for repeated tasks |

## 10) Effigy cache v1 specification (proposed)

### Configuration

```toml
[cache]
enabled = true

[cache.local]
enabled = true
dir = ".effigy/cache"  # Default
max_size = "1GB"       # LRU eviction

[cache.remote]
enabled = false
backend = "s3"         # s3 | gcs | azure | http
bucket = ""
prefix = "effigy/"
region = "us-east-1"
# Credentials from env: AWS_ACCESS_KEY_ID, etc.
```

### Per-task cache config

```toml
[tasks.build.cache]
enabled = true
inputs = ["src/**", "Cargo.*"]
outputs = ["target/release/app"]
env = ["RUSTFLAGS"]  # Env vars that affect output
```

### Commands

```bash
effigy cache inspect          # Show cache statistics
effigy cache invalidate build # Invalidate specific task
effigy cache invalidate --all # Clear all caches
effigy cache gc               # Run garbage collection
```

## Next Task

1. Create concept document: `docs/concepts/caching-strategy.md`
2. Create implementation roadmap for caching improvements
3. Begin Track 03: Watch Mode and File Monitoring

