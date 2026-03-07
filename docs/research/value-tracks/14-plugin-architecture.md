# Track 14: Plugin and Extension Architecture

Status: Draft
Value track: Plugin and Extension Architecture (ESLint, Bazel rules)
Created: 2026-03-07
Tools covered: ESLint plugins, Bazel rules, Vite plugins

## 1) Synthesis

### Common Patterns

| Pattern | ESLint | Bazel | Vite | Description |
|---------|--------|-------|------|-------------|
| API style | Function rules | Starlark rules | Function hooks | Code-based extension |
| Configuration | JS/JSON configs | Starlark files | JS config | Declarative registration |
| Discovery | npm naming | Bzlmod registry | npm packages | Registry/discovery |
| Execution | In-process | In-process | In-process | Same process |
| Sandboxing | None | Hermetic | None | Isolation level |
| Versioning | Semver | Semver | Semver | Standard versioning |

### Key Insights

**Two plugin models:**

| Model | Examples | Best For | Complexity |
|-------|----------|----------|------------|
| Function-based | ESLint, Vite | Simple extensions | Low |
| Rule-based | Bazel | Complex build logic | High |

Effigy likely needs function-based for task runner use case.

**Configuration vs. code:**

All tools allow both:
- **Config-driven**: Register plugins in config file
- **Code-driven**: Import and use programmatically

Config-driven is more common for end users.

**Ecosystem growth factors:**

1. **Simple API**: Easy to get started
2. **Documentation**: Clear examples and guides
3. **Naming conventions**: Easy discovery
4. **Registry**: Central discovery
5. **Stability**: API doesn't break

### What Works

**ESLint patterns:**
- Simple rule functions
- Configuration extension
- Conventional npm naming
- Rich ecosystem

**Bazel patterns:**
- Declarative rule definitions
- Hermetic execution
- Toolchains for abstraction
- Bzlmod for dependencies

**Vite patterns:**
- Hook-based plugins
- Rollup compatibility
- Configuration in JS
- Large ecosystem

### What Doesn't

**Anti-patterns:**
- Complex plugin APIs (steep learning curve)
- Breaking changes (ecosystem churn)
- Version conflicts (dependency hell)
- Poor documentation (adoption barrier)

**Pain points:**
- ESLint: Config hell, flat config migration
- Bazel: Steep learning curve, verbosity
- Vite: Plugin ordering, config complexity

## 2) Cross-Tool Capabilities Matrix

| Capability | ESLint | Bazel | Vite | Effigy Should |
|------------|--------|-------|------|---------------|
| **API complexity** | Low | High | Low | Low-Medium |
| **Configuration** | JS/JSON | Starlark | JS/TS | TOML + simple |
| **Execution** | In-process | In-process | In-process | In-process |
| **Sandboxing** | None | Hermetic | None | Optional |
| **Discovery** | npm | BCR | npm | GitHub/registry |
| **Versioning** | Semver | Semver | Semver | Semver |
| **Documentation** | Excellent | Good | Good | Required |

## 3) Extension patterns

### Pattern 1: Task templates

Extensions provide reusable task definitions:

```toml
# effigy.toml
[extensions]
node = "github:effigy/extension-node@v1"
rust = "github:effigy/extension-rust@v1"

[[task]]
template = "node:build"
name = "build"
```

### Pattern 2: Hook-based

Extensions hook into lifecycle events:

```toml
# effigy.toml
[extensions.cache-s3]
hook = "pre-cache"
provider = "s3"
bucket = "my-cache"
```

Lifecycle events:
- `pre-init`: Before initialization
- `post-init`: After initialization
- `pre-task`: Before task execution
- `post-task`: After task execution
- `pre-cache`: Before cache operation
- `post-cache`: After cache operation

### Pattern 3: Command plugins

Extensions add new CLI commands:

```toml
# effigy.toml
[extensions.doctor]
command = "doctor"
module = "github:effigy/extension-doctor"
```

```bash
effigy doctor  # New command from extension
```

## 4) Sandboxing Comparison

| Approach | Security | Performance | Complexity | Use Case |
|----------|----------|-------------|------------|----------|
| In-process | Low | Fast | Low | Trusted extensions |
| WASM | Medium | Medium | Medium | Third-party |
| Out-of-process | High | Slow | High | Untrusted |
| Hermetic | Very high | Slow | Very high | Reproducible builds |

For Effigy (task runner): In-process or WASM for third-party.

## 5) Gaps and Opportunities

### Gaps in current tools

1. **Config hell**: Complex configuration chains
2. **Version conflicts**: Plugin dependency issues
3. **Migration pain**: API changes break ecosystem
4. **Security**: Limited sandboxing options

### Opportunities for Effigy

1. **Simple API**: Easy to write extensions
2. **Config simplicity**: TOML-based registration
3. **API stability**: Versioned, stable API
4. **Optional sandboxing**: WASM for untrusted
5. **GitHub distribution**: No central registry needed

## 6) Recommendations for Effigy

### Core Principle

> Extensions should be simple to write, easy to discover, and stable over time. Prioritize developer experience over power.

### Specific Recommendations

**1. Extension API**

```rust
// Rust trait for extensions
pub trait Extension {
  fn name(&self) -> &str;
  fn version(&self) -> &str;
  
  // Register tasks
  fn tasks(&self) -> Vec<Task>;
  
  // Hook into lifecycle
  fn hooks(&self) -> Vec<Hook>;
}
```

Or simpler: TOML-based task templates.

**2. Extension Registration**

```toml
# effigy.toml
[extensions]
# GitHub releases
node = "github:effigy/extension-node@v1.2.0"

# Local path
local-tasks = { path = "./extensions/local" }

# Inline (simple)
[extensions.notify]
hook = "post-task"
command = "notify-send 'Task complete'"
```

**3. Task Templates**

Extensions provide reusable patterns:

```toml
# From extension-node
[template.node.build]
description = "Build Node.js project"
command = "npm run build"
depends = ["node:install"]

[template.node.install]
description = "Install dependencies"
command = "npm ci"
```

**4. Lifecycle Hooks**

```toml
[extensions.my-hook]
hook = "pre-task"
task = "build"  # Optional: specific task
command = "echo 'Building...'"
```

Available hooks:
- `init`: Project initialization
- `pre-task`: Before any task
- `post-task`: After any task
- `pre-cache`: Before cache read/write
- `post-cache`: After cache read/write

**5. API Stability**

- Version extensions with API compatibility
- Effigy v1.x loads extensions targeting API v1
- Clear deprecation policy

**6. Security Model**

| Extension Source | Trust Level | Execution |
|-----------------|-------------|-----------|
| Official (effigy/*) | High | In-process |
| GitHub verified | Medium | In-process or WASM |
| Unknown | Low | WASM only |
| Local | User's choice | In-process |

## 7: Extension vs. Core Tradeoffs

| Feature | Core | Extension | Rationale |
|---------|------|-----------|-----------|
| Task execution | ✅ | ❌ | Core functionality |
| Caching | ✅ | ❌ | Core functionality |
| File watching | ✅ | ❌ | Core functionality |
| Language-specific tasks | ❌ | ✅ | Too many languages |
| Cloud caching | ❌ | ✅ | Provider-specific |
| Notifications | ❌ | ✅ | Platform-specific |
| IDE integrations | ❌ | ✅ | Editor-specific |
| CI providers | ❌ | ✅ | Platform-specific |

## 8: Open Questions

- Should extensions be written in Rust (compiled) or WASM (portable)?
- How to handle extension dependencies?
- What's the governance model for official extensions?
- Should there be an extension marketplace/registry?

## 9: Next Steps

1. Define extension API surface
2. Create proof-of-concept extension
3. Document extension development guide
4. Design extension distribution mechanism
5. Research Telemetry (Track 15)
