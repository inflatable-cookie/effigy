# Track 02: Caching Strategies

Status: Draft
Track: Caching Strategies
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `PERF`, `SCALE`, `ARCH`

## 1) Problem statement

How should task outputs be cached? What caching strategy balances:
- Correctness (never use stale cache)
- Performance (fast cache lookup)
- Storage efficiency (reasonable disk/network usage)
- Team sharing (CI and developers share cache)

## 2) Why this track matters to Effigy

Effigy already has caching, but the implementation should be validated against:
- Content-addressable caching (Bazel's approach)
- Timestamp-based caching (Make's approach)
- Team caching patterns (sccache, Turbo)
- Cache granularity (per-task vs. per-file)

## 3) Cross-tool comparison

| Tool | Strategy | Strengths | Failure modes | Effigy signal |
|------|----------|-----------|---------------|---------------|
| Make | Timestamps (mtime) | Simple, ubiquitous | Clock skew, false negatives, no sharing | Avoid for correctness-critical |
| Bazel | Content hashing | Correct, reproducible, remote caching | Complex, expensive hashing | Adopt content-addressable |
| Turbo | Input pattern hashing | Fast, configurable, remote caching | Missed env vars, JS-focused | Pattern-based hashing viable |
| sccache | Compiler wrapper + hash | Easy adoption, cloud storage | Compiler-only, network latency | Wrapper pattern useful |

### Caching Strategy Spectrum

**Timestamp-based (Make)**
```
Cache key: file modification time
Pros: Instant, no computation
Cons: Clock skew, content changes without mtime update, no cross-machine sharing
```

**Content hashing (Bazel)**
```
Cache key: SHA256(all inputs)
Pros: Correct, deterministic, shareable
Cons: Expensive hashing, must declare all inputs
```

**Hybrid (Turbo, sccache)**
```
Cache key: hash(configured inputs + env vars + tool version)
Pros: Configurable granularity, faster than full hashing
Cons: Missed dependencies cause incorrect cache hits
```

## 4) Repeated patterns

### Universal caching requirements

1. **Cache key computation**
   - Must uniquely identify the task and its inputs
   - Must change when inputs change
   - Should be fast to compute

2. **Storage backend**
   - Local filesystem (fast, single user)
   - Shared network storage (team sharing)
   - Cloud object storage (S3, GCS, Azure)

3. **Cache entry contents**
   - Output files
   - Exit codes
   - Stdout/stderr (for replay)

4. **Invalidation strategy**
   - TTL (time-based expiration)
   - LRU (least recently used eviction)
   - Explicit invalidation (`effigy cache invalidate`)

### Tool-specific innovations

**Bazel: Hermeticity guarantees**
- Sandboxed execution ensures no undeclared inputs
- Correctness-first approach

**Turbo: Pipeline-aware caching**
- Understands task dependencies
- Skips downstream tasks on cache hit

**sccache: Zero-config cloud**
- Just set `RUSTC_WRAPPER=sccache`
- Direct S3/GCS integration

## 5) Frontier research signals

- **Nix store**: Content-addressable at the OS level
- **Git LFS**: Large file storage patterns applicable to cache artifacts
- **CDN caching**: HTTP cache semantics for build artifacts
- **CRDTs**: Conflict-free replicated data types for distributed caching

## 6) Effigy implications

### Recommended direction

**Content-addressable caching with configurable granularity:**

1. **Default: Smart hashing**
   - Hash task configuration (command, env vars)
   - Hash declared inputs (file patterns from `effigy.toml`)
   - Not full content hashing (too expensive) nor just timestamps (not reliable)

2. **Storage: Local + remote tiers**
   ```toml
   [cache]
   enabled = true
   local_dir = ".effigy/cache"
   remote = "s3://my-bucket/effigy-cache"
   ```

3. **Invalidation: Explicit + automatic**
   - `effigy cache invalidate <task>` for explicit
   - Automatic on configuration changes
   - TTL for old entries

### Risks to avoid

1. **Don't use timestamps alone**: Clock skew and false negatives
2. **Don't require full hermeticity**: Bazel's complexity is overkill for most
3. **Don't ignore cache poisoning**: Validate cache entries
4. **Don't make cloud required**: Local-only should work well

### Evidence or prototype needed

- [ ] Benchmark: Hash computation cost vs. task execution time
- [ ] Test: Cache hit rates with different granularity strategies
- [ ] Validate: S3/GCS integration performance
- [ ] Measure: Cache size growth and eviction needs

## 7) Implementation suggestions

### Cache key structure

```rust
struct CacheKey {
    task_name: String,
    task_config_hash: Hash,  // Hash of command, working dir, env vars
    input_hash: Hash,        // Hash of input files (content or metadata)
    tool_version: String,    // Effigy version (invalidate on upgrades)
}
```

### Storage layout

```
.effigy/cache/
  v1/                          # Cache format version
    <hash_prefix>/
      <full_hash>/
        outputs/               # Output files
        exit_code              # Exit code
        stdout                 # Captured stdout
        stderr                 # Captured stderr
        metadata.json          # Task info, timestamps
```

### Remote caching protocol

Option 1: Simple HTTP PUT/GET
```
PUT /cache/<hash>  → Store cache entry
GET  /cache/<hash> → Retrieve cache entry
```

Option 2: sccache-compatible
- Use sccache's protocol for compatibility
- Could use sccache server as backend

## 8) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| Bazel caching docs | official | high | Content-addressable reference |
| Turbo caching docs | official | high | Practical implementation |
| sccache docs | official | high | Cloud storage patterns |
| Make manual | official | high | Timestamp baseline |
| Nix store docs | official | medium | Content-addressable OS |

## 9) Decision state

- [ ] `promote to concept work` — Design content-addressable cache
- [ ] `continue research` — Need more data on granularity tradeoffs
- [ ] `prototype first` — Benchmark hash strategies

**Current leaning**: Prototype first — implement configurable content-addressable caching and measure.

## Next Task

1. Draft Translation Memo 002: Caching Strategy
2. Begin Track 03: Watch Mode and File Monitoring

