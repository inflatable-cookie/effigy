# Dagger (dagger.io)

Status: Draft
Tool name: Dagger
Category: CI/CD execution engine (container-based DAG)
Owner:
Last updated: 2026-03-07
Scope: Dagger 0.x documentation, container-based pipelines, programmable CI/CD

## 1) Why this tool matters

Dagger is a programmable CI/CD engine that runs pipelines in containers. Created by the founders of Docker (Solomon Hykes), it brings container advantages (hermeticity, reproducibility) to CI/CD with a modern DAG-based execution model.

For Effigy, Dagger represents:
- Container-based task isolation
- Programmable pipelines (code, not YAML)
- DAG execution with caching at the container layer
- A modern approach to reproducible workflows

## 2) Product and era context

### Timeline

- **2021**: Dagger founded by Solomon Hykes (Docker founder)
- **2022**: Public launch, initial SDKs (Go, Node.js, Python)
- **2023**: Dagger Engine architecture solidified
- **2024**: Growing ecosystem, Cloud offering

### Design Philosophy

From Dagger documentation:

> "Programmable CI/CD"
> "Write your pipeline as code, run it anywhere"
> "Hermetic, reproducible builds"

### Target Audience

- Teams frustrated with YAML-based CI/CD
- Developers wanting type-safe pipelines
- Organizations needing reproducible builds
- Multi-cloud CI/CD users

### Positioning

Dagger is not a CI provider (like GitHub Actions), but an engine:
- Write pipelines in Go/Node.js/Python
- Run locally or in any CI provider
- Same behavior everywhere (containers ensure consistency)

## 3) Defining architectural bets

### Container-based execution

Every step runs in a container:

```go
// Dagger Go SDK
func build(ctx context.Context, client *dagger.Client) (*dagger.Container, error) {
    src := client.Host().Directory(".")
    
    return client.Container().
        From("golang:1.21").                    // Base image
        WithMountedDirectory("/src", src).      // Mount source
        WithWorkdir("/src").
        WithExec([]string{"go", "build", "."}). // Run command
        Sync(ctx)
}
```

This guarantees:
- Hermetic builds
- Reproducible environments
- Cacheable intermediate states

### DAG-based execution

Dagger builds a DAG of operations:

```
Fetch base image ──┐
                   ├──> Mount source ──> Run build ──> Export artifact
Checkout source ───┘
```

Operations are parallelized when independent, cached when inputs unchanged.

### Code-based configuration

Pipelines are written in real programming languages:

```go
// ci/main.go
func main() {
    ctx := context.Background()
    client, err := dagger.Connect(ctx)
    if err != nil {
        panic(err)
    }
    defer client.Close()

    // Define pipeline
    src := client.Host().Directory(".")
    
    test := client.Container().
        From("golang:1.21").
        WithMountedDirectory("/src", src).
        WithWorkdir("/src").
        WithExec([]string{"go", "test", "./..."})

    _, err = test.Sync(ctx)
    if err != nil {
        panic(err)
    }
}
```

Benefits:
- Type safety
- IDE support
- Code reuse
- Testing pipelines

### Caching at container layer

Dagger caches container filesystem layers:
- Each `WithExec` creates a layer
- Layers are content-addressed
- Reused across runs, across machines

```go
// This layer is cached if go.mod/go.sum unchanged
base := client.Container().
    From("golang:1.21").
    WithFile("/src/go.mod", client.Host().File("go.mod")).
    WithFile("/src/go.sum", client.Host().File("go.sum")).
    WithExec([]string{"go", "mod", "download"})  // Cached!
```

## 4) Standout strengths

- **Hermetic builds**: Containers ensure reproducibility
- **Local execution**: Run full CI pipeline on laptop
- **Type safety**: Real programming languages, not YAML
- **Caching**: Automatic layer caching
- **Vendor independence**: Run in any CI provider
- **Debuggability**: Interactive container debugging
- **Cross-platform**: Linux containers run anywhere

## 5) Chronic weaknesses and recurring costs

### Container overhead

Every step in a container:
- Image pull latency
- Container startup overhead
- Filesystem layering costs

For small/fast tasks, container overhead dominates.

### Learning curve

Programming-based pipelines require:
- Learning Dagger SDK
- Understanding container concepts
- Writing code for what YAML did declaratively

### Resource usage

Dagger Engine runs as a daemon:
- Background process
- Container image cache storage
- Memory for tracking state

### Docker dependency (or alternatives)

Requires container runtime:
- Docker
- Podman
- Containerd

Not suitable for environments without containers.

### Debugging complexity

When pipelines fail:
- Multiple abstraction layers
- Container internals to inspect
- Network of connected operations

## 6) Between-release corrections

### v0.1 → v0.2 (2022-2023)
- SDK stabilization
- Better error messages
- Improved caching

### v0.3+ (2023-2024)
- Dagger Cloud (remote caching, visualization)
- Module system for reusable components
- GraphQL API exposure

The pattern: Maturing from demo to production-ready platform.

## 7) Effigy-relevant lessons

### Adopt carefully

- **DAG execution**: Explicit dependency graphs are powerful
- **Caching granularity**: Container layers as cache units
- **Hermeticity**: Isolated execution prevents "works on my machine"
- **Local-first**: Being able to run CI locally is valuable

### Reject early

- **Container overhead**: Too heavy for simple/fast tasks
- **Code-based config**: TOML > code for task definitions
- **Container requirement**: Effigy shouldn't require Docker
- **Background daemon**: Keep Effigy simple, no daemons

### Prototype before deciding

- Dagger-style caching for container-based tasks
- DAG visualization for Effigy task graphs
- Local CI simulation (what Dagger does well)

## 8) Comparison: Dagger vs. Bazel vs. Effigy

| Aspect | Dagger | Bazel | Effigy (current) |
|--------|--------|-------|------------------|
| Isolation | Containers | Sandboxing | Process |
| Config | Code (Go/JS/Python) | Starlark | TOML |
| Caching | Container layers | Content-addressable | Basic |
| Primary use | CI/CD pipelines | Builds | Task runner |
| Learning curve | Medium | High | Low |
| Overhead | Container startup | Analysis/JVM | Minimal |

## 9) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Dagger docs](https://docs.dagger.io) | official docs | current | high | Primary reference |
| [Dagger GitHub](https://github.com/dagger/dagger) | source | current | high | Implementation |
| [Dagger blog](https://dagger.io/blog) | blog | 2022-2024 | high | Announcements |
| SDK docs (Go/Node/Python) | official docs | current | high | Usage patterns |
| Community Discord | community | ongoing | medium | User feedback |

## 10) Open questions

- How does Dagger's caching compare to Bazel's for non-container workloads?
- What's the overhead breakdown for small tasks?
- How many users run Dagger locally vs. only in CI?

## Next Task

Compare against Bazel and other DAG executors in Track 04 synthesis.

