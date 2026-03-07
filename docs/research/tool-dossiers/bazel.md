# Bazel

Status: Draft
Tool name: Bazel
Category: build system (remote execution, caching)
Owner:
Last updated: 2026-03-07
Scope: Bazel 6.x/7.x documentation, Bazel Remote Build Execution, build performance best practices

## 1) Why this tool matters

Bazel is Google's open-source build system, descended from the internal Blaze. It's the reference implementation for hermetic builds, content-addressable caching, and remote build execution. While overkill for many projects, Bazel's caching and execution model represents the state of the art.

For Effigy, Bazel represents:
- The gold standard for build caching (content-addressable, not timestamp-based)
- Remote execution architecture patterns
- Hermetic build philosophy (reproducible, deterministic)
- The complexity cost of maximum correctness

## 2) Product and era context

### Timeline

- **2008**: Blaze (internal Google build system) in production
- **2015**: Bazel open-sourced (Blaze rearranged)
- **2018**: Bazel 1.0 (stability promise)
- **2023**: Bazel 6.0 (bzlmod for external deps)
- **2023**: Bazel 7.0 (Skymeld, build event stream improvements)

### Design Philosophy

From Bazel documentation:

> "Bazel only rebuilds what is necessary"
> "Bazel wants your builds to be reproducible"
> "Bazel wants your builds to be fast"

### Target Audience

- Large monorepos (Google's monorepo is the original use case)
- Multi-language projects
- Teams prioritizing build reproducibility over simplicity
- Organizations with build infrastructure budget

## 3) Defining architectural bets

### Content-addressable caching

Bazel computes a hash of all inputs (source files, tools, dependencies) to determine if an output can be reused:

```
Action: compile foo.cc with compiler X and headers H1, H2
Hash: SHA256 of (foo.cc content + X content + H1 content + H2 content)
Cache lookup: Do we have outputs for this hash?
```

Unlike Make's timestamp comparison, this is deterministic and works across machines.

### Hermetic builds

Bazel enforces that builds only use declared inputs:
- No undeclared file system access
- No environment variable leakage (unless declared)
- No network access (sandboxed)
- Toolchain dependencies explicitly declared

This guarantees reproducibility at the cost of configuration complexity.

### Remote caching and execution

Bazel separates build logic from execution:
- **Remote caching**: Store/fetch build artifacts from a shared cache
- **Remote execution**: Execute build actions on remote workers
- **Build without the bytes**: Download only artifacts you need

### Skyframe evaluation model (DAG execution)

Skyframe is Bazel's core evaluation engine — a functional, incremental DAG evaluator.

#### Core concepts

**SkyValues**: Immutable data representing build artifacts, metadata, or computation results.
```java
// Conceptual representation
interface SkyValue {
  // Immutable result of a computation
}
```

**SkyKeys**: Unique identifiers for SkyValues.
```java
interface SkyKey {
  // Identifies a computation to perform
  // Example: FileValue.Key for "src/main.cc"
}
```

**SkyFunctions**: Pure functions that compute SkyValues from dependencies.
```java
interface SkyFunction {
  SkyValue compute(SkyKey key, Environment env);
}
```

#### DAG construction and evaluation

1. **Build the graph**: During analysis, Bazel constructs a graph of dependencies
   ```
   //:app (executable)
     └── //lib:helper (library)
           └── //lib:helper.cc (source)
           └── //lib:helper.h (header)
     └── //:main.cc (source)
   ```

2. **Evaluate bottom-up**: Start from leaves (source files), work up to targets
   - Source files: Read from disk, hash contents
   - Libraries: Compile if sources changed
   - Executables: Link if libraries changed

3. **Incremental evaluation**: Only re-evaluate nodes with changed dependencies
   ```
   If //lib:helper.h unchanged:
     Skip recompiling //lib:helper
     Skip relinking //:app (if main.cc also unchanged)
   ```

#### Parallel execution

Skyframe automatically parallelizes independent nodes:
```
// Parallel compilation
Compile A.cc ──┐
               ├──> Link executable
Compile B.cc ──┘
```

Execution model:
- Thread pool evaluates ready nodes
- Nodes are "ready" when all dependencies computed
- Dynamic scheduling based on graph structure

#### Cycle detection

Bazel detects dependency cycles during graph construction:
```python
# This BUILD file has a cycle
cc_library(name="A", deps=[":B"])
cc_library(name="B", deps=[":A"])  # Cycle!
```

Error: `cycle in dependency graph: //:A -> //:B -> //:A`

#### Comparison to Make

| Aspect | Make | Bazel (Skyframe) |
|--------|------|------------------|
| Graph construction | Implicit from rules | Explicit from analysis |
| Change detection | Timestamps | Content hashing |
| Parallelism | `-j` flag, simple parallelism | Dynamic DAG scheduling |
| Incremental | File-level | Fine-grained value-level |
| Reproducibility | Best effort | Guaranteed (hermetic) |

### Starlark configuration language

BUILD files use Starlark (Python-like), enabling:
- Custom rules (extensibility)
- Macros (code reuse)
- Aspect-oriented programming (cross-cutting concerns)

This power adds significant learning curve.

## 4) Standout strengths

- **Correct incremental builds**: Content hashing eliminates false negatives/positives
- **Remote caching**: Share build artifacts across team, CI, and developers
- **Remote execution**: Distribute builds across workers
- **Hermeticity**: Reproducible builds, no "works on my machine"
- **Multi-language**: Single tool for C++, Java, Go, Python, etc.
- **Query language**: Analyze dependency graph (`bazel query`)
- **Build event protocol**: Structured build progress for tooling integration

## 5) Chronic weaknesses and recurring costs

### Configuration complexity

Simple C++ build requires:

```python
# WORKSPACE - defines external dependencies
# BUILD - defines targets
# .bazelrc - configuration
# Toolchain configuration for hermetic builds
```

Learning curve is measured in weeks, not hours.

### JVM startup cost

Bazel is a Java application:
- ~2-5 second startup time
- Memory usage measured in gigabytes for large repos
- Server mode helps but doesn't eliminate cost

### File system watching limitations

Bazel's watch mode (`ibazel`) is an external tool, not core:
- Additional complexity
- Not as polished as built-in watch modes

### Vendor ecosystem complexity

External dependencies via:
- **WORKSPACE**: Git repositories, HTTP archives
- **bzlmod** (new): Module system similar to npm/cargo

Both require significant configuration compared to language-native package managers.

### IDE integration gaps

While improving, IDE integration requires:
- Language servers that understand Bazel
- Build event protocol consumers
- Custom project importers

## 6) Between-release corrections

### Bazel 4.0 → 5.0 (2022)
- Java 11 minimum (dropping Java 8)
- Improved remote execution performance

### Bazel 6.0 (2023)
- **bzlmod** enabled by default (new external dependency system)
- Addresses WORKSPACE complexity complaints

### Bazel 7.0 (2023)
- **Skymeld**: Faster incremental builds
- **Build without the bytes**: Don't download unused outputs
- Improved remote execution stability

The pattern: Bazel is gradually reducing complexity barriers while maintaining correctness guarantees.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Content-addressable caching**: Consider for Effigy's cache; more reliable than timestamps
- **Remote caching concept**: Valuable for team workflows
- **Hermeticity principles**: Documented dependencies are good
- **Parallel execution**: Design for concurrency from start

### Reject early

- **Configuration complexity**: Effigy should work out of the box, not require weeks of setup
- **JVM-style overhead**: Keep Effigy fast and lightweight (Rust helps)
- **Custom rule languages**: TOML configuration, not a programming language
- **Hermeticity enforcement**: Optional best practice, not required

### Prototype before deciding

- Content-addressable hashing for Effigy cache vs. current timestamp approach
- Remote cache integration (S3-compatible storage)
- Cache granularity (per-task vs. per-file)

## 8) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Bazel Documentation](https://bazel.build/docs) | official docs | 7.x | high | Primary reference |
| [Bazel Remote Caching](https://bazel.build/remote/caching) | official docs | 7.x | high | Cache implementation |
| [Bazel Remote Execution](https://bazel.build/remote/rbe) | official docs | 7.x | high | RBE details |
| [Build Performance Guide](https://bazel.build/docs/build-performance) | official docs | 7.x | high | Optimization |
| [Bazel Blog](https://blog.bazel.build/) | blog | ongoing | high | Release notes |
| [Build Event Protocol](https://bazel.build/remote/bep) | official docs | 7.x | high | Tooling integration |
| [Skyframe paper](https://bazel.build/docs/skyframe.html) | paper | 2015 | medium | Internal design |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 9) Open questions

- What percentage of Bazel users actually use remote caching vs. just local caching?
- How does Bazel's complexity cost compare to its benefits for medium-sized projects?
- Could Effigy implement a subset of Bazel's caching model without the complexity?

## Next Task

Compare against Turbo and sccache in Track 02 synthesis on caching strategies.

