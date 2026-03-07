# Translation Memo 011: Remote Execution Strategy

Status: Draft
Memo: 011
Owner: Research
Last updated: 2026-03-07
Related track: Track 11 — Remote Execution and Distributed Builds

## 1) Effigy problem statement

Effigy may need remote/distributed capabilities for:
- Team caching (share build artifacts)
- CI/CD optimization
- Build analytics

Research validates starting simple, scaling as needed.

## 2) External evidence summary

From comparative analysis of Bazel RBE, BuildBuddy, and sccache:

**Bazel RBE**:
- Full remote execution protocol
- Massive scale potential
- High complexity
- For very large organizations

**BuildBuddy**:
- Managed service
- Easy setup
- Subscription cost
- Bazel-specific

**sccache**:
- Simple compiler caching
- S3-compatible
- Easy adoption
- Limited scope

**Patterns**:
- Remote caching provides most value
- Full RBE is overkill for most
- Multiple backend support important
- Start simple, scale later

## 3) Recommendation

**Three-phase approach:**

### Phase 1: Remote caching (implement now)

```toml
[cache.remote]
enabled = true
backend = "s3"  # s3 | gcs | azure | http
bucket = "my-team-cache"
```

Simple S3-compatible remote cache.

### Phase 2: Build analytics (optional)

```toml
[analytics]
enabled = true
endpoint = "https://analytics.example.com"
```

Track build times, cache hit rates.

### Phase 3: Remote execution (future consideration)

Only if users request and have infrastructure:
```toml
[execution.remote]
enabled = true
endpoint = "..."
```

### Not recommended now

- Full RBE implementation: Too complex
- Build farm requirement: Overkill
- Vendor-specific integration: Lock-in

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Limited scope | Only caching, not execution | Covers 80% of use case |
| Backend complexity | Multiple storage backends | Generic interface |
| No analytics initially | Less visibility | Add later |

## 5) What must be true before adoption

- [x] S3/GCS/Azure APIs available
- [ ] Remote cache implementation
- [ ] Cache key design validation
- [ ] Performance testing

## 6) Required prototype or validation work

**Phase 1: S3-compatible cache**
- [ ] Implement RemoteCache trait
- [ ] S3 backend
- [ ] Performance benchmark
- [ ] Cache hit rate measurement

**Phase 2: Additional backends**
- [ ] GCS backend
- [ ] Azure backend
- [ ] HTTP generic backend

**Phase 3: Analytics (optional)**
- [ ] Build timing tracking
- [ ] Cache hit rate reporting
- [ ] Dashboard/endpoint

## 7) Promotion target

- [ ] `concept contract work` — Document cache strategy
- [ ] `roadmap execution planning` — Implementation roadmap
- [ ] `watch only` — Full RBE for later
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| Bazel RBE dossier | high | Protocol complexity |
| BuildBuddy dossier | high | Managed service value |
| sccache dossier | high | Simple caching works |
| Track 11 synthesis | high | Start simple |

## 9) Implementation plan

### Cache trait

```rust
#[async_trait]
pub trait RemoteCache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<()>;
}
```

### S3 implementation

```rust
pub struct S3Cache {
    client: aws_sdk_s3::Client,
    bucket: String,
}

#[async_trait]
impl RemoteCache for S3Cache {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let result = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;
        // ...
    }
    // ...
}
```

### Configuration

```toml
[cache.remote]
enabled = true
backend = "s3"
bucket = "effigy-cache"
region = "us-east-1"
# Credentials from environment
```

## 10: Cache key design

```rust
pub fn compute_cache_key(
    task: &Task,
    inputs: &InputHash,
    env_vars: &[String],
) -> String {
    let mut hasher = Sha256::new();
    
    // Task configuration
    hasher.update(&task.command);
    hasher.update(&task.working_dir);
    
    // Input files
    hasher.update(&inputs.hash);
    
    // Environment variables that affect output
    for var in env_vars {
        if let Ok(value) = env::var(var) {
            hasher.update(var.as_bytes());
            hasher.update(value.as_bytes());
        }
    }
    
    hex::encode(hasher.finalize())
}
```

## Next Task

1. Create concept document: `docs/concepts/remote-caching.md`
2. Begin Track 12: CI/CD Integration

