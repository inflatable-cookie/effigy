# sccache

Status: Draft
Tool name: sccache (Shared Compilation Cache)
Category: compiler cache (caching build artifacts)
Owner:
Last updated: 2026-03-07
Scope: sccache 0.x documentation, cloud storage backends, Rust ecosystem usage

## 1) Why this tool matters

sccache is Mozilla's compiler cache, a Rust rewrite of ccache with support for cloud storage backends (S3, GCS, Azure Blob). It's the standard for distributed compiler caching in the Rust ecosystem and increasingly used for C/C++.

For Effigy, sccache represents:
- Practical caching without build system complexity
- Cloud storage integration patterns
- The "wrapper" approach to caching
- Rust ecosystem tooling best practices

## 2) Product and era context

### Timeline

- **2015**: sccache created at Mozilla
- **2016**: Rewritten in Rust (original was Python)
- **2018**: Cloud storage backends added
- **2020**: Mozilla transfers to community maintenance
- **2023-2024**: Active development, S3/GCS/Azure improvements

### Design Philosophy

From the README:

> "sccache is a ccache-like compiler caching tool"
> "It uses the same command-line interface as ccache"
> "Supports cloud storage for shared caches"

### Target Audience

- Rust developers (primary use case)
- C/C++ developers needing distributed caching
- CI/CD pipelines wanting shared compiler caches
- Teams with S3/GCS/Azure infrastructure

## 3) Defining architectural bets

### Compiler wrapper approach

sccache doesn't require build system changes:

```bash
# Instead of: rustc main.rs
# Use: sccache rustc main.rs

# Or set wrapper:
export RUSTC_WRAPPER=sccache
cargo build  # Uses sccache automatically
```

This makes adoption easy — no BUILD files, no configuration.

### Cloud storage backends

sccache supports multiple storage backends:
- Local disk
- Redis
- Amazon S3
- Google Cloud Storage
- Azure Blob Storage
- memcached

Configuration via environment variables:
```bash
export SCCACHE_BUCKET=my-cache-bucket
export SCCACHE_REGION=us-east-1
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
```

### Hash-based caching

sccache hashes:
- Compiler binary
- Compiler arguments
- Input file contents
- Environment variables affecting compilation

Hash lookup → cache hit → return cached output

### No server required (mostly)

Unlike Bazel's remote execution, sccache:
- Runs locally
- Talks directly to storage
- No build farm to manage

(Has optional scheduler for distributed compilation, but caching is standalone)

## 4) Standout strengths

- **Easy adoption**: Just set `RUSTC_WRAPPER=sccache`
- **Cloud storage**: Share cache across CI and developers
- **Multi-language**: Rust, C, C++, CUDA
- **No infrastructure**: No servers to manage (direct to S3)
- **Statistics**: `sccache --show-stats` shows hit rates
- **Local + remote**: Two-tier caching

## 5) Chronic weaknesses and recurring costs

### Cache invalidation complexity

When to invalidate is tricky:
- Compiler updates
- Standard library changes
- Environment variable changes
- Header file changes (for C/C++)

sccache handles some of this, but cache poisoning is possible.

### Network latency for cache misses

Cloud storage adds latency:
- Local cache miss → network round-trip
- Large artifacts → slow download
- Not ideal for low-latency local builds

### Limited to compilation

sccache caches compiler outputs, not:
- Linking
- Test execution
- Code generation
- General task execution

### Configuration complexity for cloud

Setting up cloud storage requires:
- Bucket creation
- IAM permissions
- Credential management
- Region selection

More work than local-only caching.

## 6) Between-release corrections

### Early versions → 0.3.x (2020)
- Improved Rust support
- Better error handling

### 0.4.x → 0.5.x (2022-2023)
- WebDAV support added
- Improved Windows support
- Better S3/GCS compatibility

### 0.6.x+ (2023-2024)
- Direct mode (faster local caching)
- GCS improvements
- memcached backend

The pattern: sccache is adding backends and improving reliability.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Wrapper pattern**: Effigy could support `RUSTC_WRAPPER`-style integration
- **Cloud storage**: S3/GCS/Azure support for team caching
- **Hash-based**: Content hashing > timestamps
- **Statistics**: Show cache hit rates to users
- **Two-tier**: Local + remote cache hierarchy

### Reject early

- **Compiler-only focus**: Effigy needs general task caching
- **Environment variable config**: TOML config is clearer
- **No visibility**: Users don't see what's cached vs. built

### Prototype before deciding

- sccache integration with `cargo` tasks in Effigy
- Cloud storage backend for Effigy's cache
- Cache hit rate reporting in `effigy doctor`

## 8) Effigy Integration Possibilities

### Option 1: Native sccache integration

Effigy automatically uses sccache for Rust tasks:
```toml
[cache]
use_sccache = true
sccache_bucket = "my-cache"
```

### Option 2: Learn from sccache patterns

Implement similar patterns in Effigy's cache:
- S3-compatible storage
- Hash-based lookup
- Statistics reporting

### Option 3: Wrapper compatibility

Ensure Effigy works with sccache as `RUSTC_WRAPPER`:
```bash
RUSTC_WRAPPER=sccache effigy build
```

## 9) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [sccache README](https://github.com/mozilla/sccache) | official docs | current | high | Primary reference |
| [sccache documentation](https://github.com/mozilla/sccache/blob/main/docs/) | official docs | current | high | Detailed setup |
| [Rust sccache guide](https://doc.rust-lang.org/cargo/guide/build-cache.html) | official docs | current | high | Cargo integration |
| GitHub releases/changelog | changelog | 2015-2024 | high | Version history |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 10) Open questions

- What's the typical cache hit rate for Rust projects using sccache?
- How does sccache handle Docker/containerized builds?
- What's the cost breakdown for S3-backed caching at scale?

## Next Task

Compare against Bazel and Turbo in Track 02 synthesis.

