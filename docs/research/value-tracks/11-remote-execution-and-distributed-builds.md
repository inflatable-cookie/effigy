# Track 11: Remote Execution and Distributed Builds

Status: Draft
Track: Remote Execution and Distributed Builds
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `SCALE`, `INFRA`, `PERF`

## 1) Problem statement

How can builds scale beyond a single machine? What patterns enable:
- Parallel execution across many workers
- Shared caching across organization
- Reproducible builds at scale
- Fast CI/CD pipelines

## 2) Why this track matters to Effigy

Effigy may need to support:
- Remote caching for team workflows
- Distributed task execution
- Build analytics
- CI/CD optimization

Research validates:
- Remote execution architectures
- Distributed caching patterns
- Build farm designs
- Managed service tradeoffs

## 3) Cross-tool comparison

| Tool | Approach | Scale | Complexity | Cost |
|------|----------|-------|------------|------|
| Bazel RBE | Protocol-based | Massive | High | Self-managed |
| BuildBuddy | Managed service | Large | Low | Subscription |
| Buildkite | Hybrid CI | Medium | Medium | Per-user |
| sccache | Simple cache | Small | Low | Free |

### Remote Execution Spectrum

**Simple caching (sccache)**
- Shared compiler cache
- Easy setup
- Limited to compilation

**Managed service (BuildBuddy)**
- Full RBE platform
- No infrastructure
- Subscription cost

**Self-hosted (Buildbarn)**
- Full control
- Complex setup
- Infrastructure cost

**Protocol-based (Bazel RBE)**
- Standardized API
- Multiple implementations
- Maximum flexibility

## 4) Repeated patterns

### Universal remote execution needs

1. **Content-addressable storage**
   - Store inputs/outputs by hash
   - Deduplication
   - Integrity verification

2. **Action-based execution**
   - Decompose work into actions
   - Content-addressed
   - Cacheable

3. **Worker management**
   - Worker pools
   - Task scheduling
   - Sandboxing

4. **Caching layers**
   - Local cache
   - Remote cache
   - Action cache

### Tool-specific innovations

**Bazel: RBE protocol**
- Standardized API
- Multiple implementations
- Ecosystem interoperability

**BuildBuddy: Managed service**
- Zero infrastructure
- Analytics included
- Quick setup

**sccache: Simplicity**
- Compiler wrapper
- S3-compatible
- Easy adoption

## 5) Frontier research signals

- **Serverless builds**: Cloud functions as workers
- **Wasm-based execution**: Sandboxed, portable workers
- **Edge caching**: CDN-style build artifact distribution
- **AI-powered optimization**: Predictive build scheduling

## 6) Effigy implications

### Recommended direction

**Start simple, scale as needed:**

1. **Phase 1: Remote caching** (near-term)
   ```toml
   [cache.remote]
   enabled = true
   backend = "s3"  # or gcs, azure
   bucket = "my-team-cache"
   ```

2. **Phase 2: Build analytics** (optional)
   ```toml
   [analytics]
   enabled = true
   backend = "http"
   endpoint = "https://analytics.example.com"
   ```

3. **Phase 3: Remote execution** (future consideration)
   ```toml
   [execution.remote]
   enabled = true
   endpoint = "https://buildfarm.example.com"
   # Only if user has infrastructure
   ```

### Risks to avoid

1. **Premature complexity**: Don't build RBE before needed
2. **Vendor lock-in**: Support multiple backends
3. **Infrastructure requirement**: Should work locally

### Evidence or prototype needed

- [ ] Remote cache implementation
- [ ] S3/GCS/Azure compatibility
- [ ] Cache hit rate tracking

## 7) Implementation suggestions

### Remote cache protocol

```rust
pub trait RemoteCache {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<()>;
}

pub struct S3Cache {
    bucket: String,
    client: S3Client,
}

pub struct GcsCache {
    bucket: String,
    client: GcsClient,
}
```

### Configuration

```toml
[cache.remote]
enabled = true
backend = "s3"  # s3 | gcs | azure | http

[cache.remote.s3]
bucket = "effigy-cache"
region = "us-east-1"
# Credentials from env: AWS_ACCESS_KEY_ID, etc.

[cache.remote.http]
endpoint = "https://cache.example.com"
auth_token = "..."
```

### Cache key design

```rust
pub fn cache_key(task: &Task, inputs: &InputHash) -> String {
    // Hash of:
    // - Task configuration
    // - Input files
    // - Environment variables that affect output
    format!("{}-{}", task.name, inputs.hash())
}
```

## 8: Comparison: Approaches

| Approach | Pros | Cons | Effigy |
|----------|------|------|--------|
| sccache-style | Simple | Limited | ✅ Start here |
| Bazel RBE | Full-featured | Complex | ⚠️ Future |
| BuildBuddy | Easy | Cost/vendor | ⚠️ Optional |

## 9: Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| Bazel RBE dossier | high | Protocol patterns |
| BuildBuddy dossier | high | Managed service |
| sccache dossier | high | Simple caching |

## 10: Decision state

- [ ] `promote to concept work` — Document remote cache strategy
- [ ] `continue research` — Sufficient for now
- [ ] `prototype first` — Test S3 cache implementation

**Current leaning**: Prototype first — implement S3-compatible remote cache.

## Next Task

1. Draft Translation Memo 011: Remote Execution Strategy
2. Begin Track 12: CI/CD Integration

