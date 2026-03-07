# Bazel Remote Execution

Status: Draft
Tool name: Bazel Remote Execution (RBE)
Category: build system (remote execution, distributed builds)
Owner:
Last updated: 2026-03-07
Scope: Bazel Remote Execution API, build farm architecture, distributed caching

## 1) Why this tool matters

Bazel Remote Execution (RBE) is Google's protocol for distributed builds. It enables:
- Running build actions on remote workers
- Sharing build cache across organization
- Massive parallelization
- Reproducible builds at scale

For Effigy, RBE represents:
- Remote execution architecture patterns
- Build farm design
- Distributed caching protocols
- Enterprise-scale build strategies

## 2) Product and era context

### Timeline

- **2016**: Bazel open-sourced with remote execution support
- **2017**: Remote Execution API v1 published
- **2018-2020**: BuildBuddy, EngFlow, and other RBE implementations
- **2021-2024**: API v2, performance improvements, wider adoption

### Design Philosophy

From Bazel documentation:

> "Build anywhere, cache everywhere"
> "Hermetic, reproducible remote builds"
> "Scale to thousands of workers"

### Target Audience

- Large organizations with monorepos
- Teams needing fast CI/CD
- Companies with build infrastructure investment
- Organizations prioritizing reproducibility

### Ecosystem

- **Bazel**: Client that speaks RBE protocol
- **BuildBuddy**: Managed RBE service
- **EngFlow**: Enterprise RBE platform
- **Buildbarn**: Open-source RBE implementation

## 3) Defining architectural bets

### Remote Execution API

Standardized protocol for remote builds:

```protobuf
// Execute an action remotely
rpc Execute(ExecuteRequest) returns (stream ExecuteResponse);

// Get action result from cache
rpc GetActionResult(GetActionResultRequest) returns (ActionResult);

// Update action cache
rpc UpdateActionResult(UpdateActionResultRequest) returns (ActionResult);
```

This enables:
- Multiple client implementations
- Multiple server implementations
- Interoperability

### Action-based execution

Builds are decomposed into actions:

```protobuf
message Action {
  // Command to execute
  Command command;
  
  // Input files (content-addressed)
  Digest input_root;
  
  // Environment variables
  map<string, string> environment_variables;
}
```

Actions are:
- Content-addressed (hash of inputs)
- Cacheable
- Reproducible

### Content-addressable storage (CAS)

All inputs/outputs stored by hash:

```protobuf
rpc BatchReadBlobs(BatchReadBlobsRequest) returns (BatchReadBlobsResponse);
rpc BatchUpdateBlobs(BatchUpdateBlobsRequest) returns (BatchUpdateBlobsResponse);
```

Benefits:
- Deduplication
- Integrity verification
- Cache sharing

### Worker pools

Remote workers execute actions:

```
Bazel Client → RBE Scheduler → Worker Pool
                    ↓
              CAS (cache)
```

Workers:
- Pull actions from queue
- Execute in sandbox
- Upload results to CAS
- Report completion

## 4) Standout strengths

- **Massive parallelism**: Thousands of workers
- **Cache sharing**: Organization-wide cache
- **Reproducibility**: Hermetic, deterministic builds
- **Scalability**: Handles largest monorepos
- **Standardized**: Open API, multiple implementations

## 5) Chronic weaknesses and recurring costs

### Infrastructure complexity

RBE requires:
- Build farm (worker pool)
- CAS storage (often distributed)
- Scheduler/queue
- Network configuration

Not a "drop in" solution.

### Network overhead

Every action requires:
- Upload inputs to CAS
- Download outputs from CAS
- API calls for queue/check

Network latency matters.

### Cost

Running RBE infrastructure:
- Compute for workers
- Storage for CAS
- Network egress

Managed solutions (BuildBuddy, EngFlow) charge by usage.

### Debugging complexity

When remote builds fail:
- Can't easily SSH to worker
- Logs distributed across farm
- Reproducing locally may differ

## 6) Between-release corrections

### RBE v1 (2017)
- Initial protocol
- Basic execution

### RBE v2 (2020+)
- Improved performance
- Better caching
- Compression support

The pattern: Maturing protocol, more implementations.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Remote execution concept**: For large-scale CI
- **Content-addressable storage**: CAS pattern
- **Action-based caching**: Granular caching
- **Worker pool model**: Distributed execution

### Reject early

- **Full RBE complexity**: Overkill for most
- **Build farm requirement**: Effigy should work locally
- **Protocol implementation**: Use existing if needed

### Prototype before deciding

- Simple remote cache (S3-compatible)
- Basic distributed execution
- CAS-like content addressing

## 8) Comparison: Local vs. Remote Execution

| Aspect | Local | Remote (RBE) |
|--------|-------|--------------|
| Speed | Limited by local CPU | Massively parallel |
| Setup | None | Complex infrastructure |
| Cost | Hardware only | Infrastructure + compute |
| Debugging | Easy | Harder |
| Cache | Local only | Shared organization-wide |

**For Effigy**: Start with remote caching, consider execution later.

## 9) Effigy Application (Future)

### Phase 1: Remote caching (near-term)

```toml
[cache.remote]
enabled = true
backend = "s3"  # or gcs, azure, http
bucket = "my-team-cache"
```

Simpler than full RBE, provides value.

### Phase 2: Remote execution (future)

If needed, could implement:
```toml
[execution.remote]
enabled = true
endpoint = "https://buildfarm.example.com"
credentials = { ... }
```

But local execution is fine for most.

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Remote Execution API](https://github.com/bazelbuild/remote-apis) | specification | current | high | Protocol definition |
| [Bazel RBE docs](https://bazel.build/remote/rbe) | official docs | current | high | User guide |
| [BuildBuddy](https://www.buildbuddy.io/) | service | current | high | Managed RBE |
| [Buildbarn](https://github.com/buildbarn) | open source | current | high | Self-hosted |

## 11) Open questions

- What's the break-even point for RBE vs. local builds?
- How do teams justify RBE infrastructure cost?
- What's the debugging experience like at scale?

## Next Task

Compare against BuildBuddy and other tools in Track 11 synthesis on remote execution.

