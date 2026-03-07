# Translation Memo 014: Plugin and Extension Architecture

**Status:** Draft  
**Track:** 14 - Plugin and Extension Architecture  
**Tools:** ESLint plugins, Bazel rules, Vite plugins  
**Date:** 2026-03-07  
**Related:** Translation Memo 013 (IDE Integration)

## Executive Summary

This memo translates Track 14 research findings into concrete implementation guidance for Effigy's extension architecture. The key insight: **Extensions should be simple to write (function-based), easy to discover (GitHub distribution), and stable over time (versioned API). Prioritize developer experience over power.**

## Research Summary

### ESLint Plugin System
- **Strengths**: Simple function-based API, huge ecosystem, configuration extension
- **Weaknesses**: Config hell, flat config migration pain, version conflicts
- **Pattern**: Export rules as functions, npm distribution

### Bazel Rules System
- **Strengths**: Hermetic builds, declarative Starlark, rich rule API
- **Weaknesses**: Steep learning curve, verbose, complex migration
- **Pattern**: Rules define input/output graph, Starlark language

### Common Pattern
Successful plugin systems share:
1. Simple APIs (easy to start)
2. Clear documentation (adoption)
3. Conventional naming (discovery)
4. Stable APIs (ecosystem health)

## Core Principles

### 1. Simple Over Powerful

ESLint's simple function API enabled 1000+ plugins. Bazel's rich API has fewer extensions due to complexity.

### 2. Config-Driven Registration

Users register extensions in config, not code:

```toml
# effigy.toml
[extensions]
node = "github:effigy/extension-node@v1"
```

Not:
```javascript
// Too complex for task runner
import nodeExtension from 'effigy-extension-node';
effigy.register(nodeExtension);
```

### 3. API Stability

Promise: Extensions targeting API v1 work with all Effigy 1.x releases.

### 4. Optional Sandboxing

- Trusted extensions: In-process (fast)
- Untrusted extensions: WASM (sandboxed)

## Proposed Implementation

### Phase 1: Task Templates

**Simplest extension mechanism: task templates.**

Extensions provide reusable task definitions:

```toml
# extension-node/effigy-extension.toml
[extension]
name = "node"
version = "1.0.0"
api-version = "1"

[template.install]
description = "Install Node.js dependencies"
command = "npm ci"
inputs = ["package.json", "package-lock.json"]
outputs = ["node_modules/"]

[template.build]
description = "Build Node.js project"
command = "npm run build"
depends = ["node:install"]
inputs = ["src/"]
outputs = ["dist/"]

[template.test]
description = "Run Node.js tests"
command = "npm test"
depends = ["node:install"]
```

**Usage:**

```toml
# Project's effigy.toml
[extensions]
node = "github:effigy/extension-node@v1"

[[task]]
template = "node:build"  # Use template
name = "build"           # Override name
command = "npm run build:prod"  # Override command
```

### Phase 2: Lifecycle Hooks

**Hook into task execution:**

```toml
# effigy.toml
[extensions.notify]
hook = "post-task"
command = "notify-send 'Task complete'"

[extensions.metrics]
hook = "post-task"
command = "effigy-metrics log"
```

**Hook types:**

| Hook | When | Use Case |
|------|------|----------|
| `init` | Project init | Setup templates |
| `pre-task` | Before task | Setup, validation |
| `post-task` | After task | Notifications, metrics |
| `pre-cache` | Before cache | Cache key customization |
| `post-cache` | After cache | Cache analytics |

**Conditional hooks:**

```toml
[extensions.notify-fail]
hook = "post-task"
when = "failure"  # Only on failure
command = "notify-send 'Build failed'"
```

### Phase 3: External Extensions

**Load from GitHub:**

```toml
[extensions]
# GitHub release
node = "github:effigy/extension-node@v1.2.0"

# GitHub branch
experimental = "github:user/extension@main"

# Local path
local = { path = "./extensions/local" }

# Git URL
private = { git = "https://github.com/org/extension", tag = "v1.0.0" }
```

**Extension loading:**

1. Parse `effigy.toml`
2. Download/cache extensions
3. Load templates and hooks
4. Merge with local configuration

### Phase 4: Extension API (Future)

**For complex extensions:**

```rust
// Rust API (for compiled extensions)
pub trait Extension {
  fn name(&self) -> &str;
  fn version(&self) -> &str;
  fn api_version(&self) -> &str;
  
  fn register(&self, registry: &mut Registry);
}

// WASM API (for portable extensions)
#[no_mangle]
pub extern "C" fn effigy_extension_register() -> *mut Extension {
  // Return extension instance
}
```

**Use cases:**
- Complex build logic
- Custom caching backends
- Platform integrations
- IDE protocol adapters

### Phase 5: Security Model

**Trust levels:**

| Source | Trust | Execution |
|--------|-------|-----------|
| `effigy/*` | High | In-process |
| GitHub verified | Medium | In-process (opt-in) or WASM |
| Unknown | Low | WASM only |
| Local | User's choice | Configurable |

**Configuration:**

```toml
[settings.extensions]
sandbox = "auto"  # auto | always | never
allow-remote = true
allowed-sources = ["github:effigy/*", "github:trusted-org/*"]
```

## Implementation Priorities

| Priority | Feature | Rationale |
|----------|---------|-----------|
| P1 | Task templates | Most common use case |
| P1 | TOML-based extensions | Simple distribution |
| P2 | Lifecycle hooks | Extensibility |
| P2 | GitHub loading | Distribution |
| P3 | Extension caching | Performance |
| P3 | WASM sandbox | Security |
| P4 | Rust/WASM API | Complex extensions |

## Extension Examples

### Example 1: Node.js Extension

```toml
# github.com/effigy/extension-node/effigy-extension.toml
[extension]
name = "node"
version = "1.0.0"

[template.install]
description = "Install Node.js dependencies"
command = "npm ci"
inputs = ["package.json", "package-lock.json"]
outputs = ["node_modules/.package-lock.json"]

[template.build]
description = "Build Node.js project"
command = "npm run build"
depends = ["node:install"]
```

### Example 2: Notifications Extension

```toml
# github.com/effigy/extension-notify/effigy-extension.toml
[extension]
name = "notify"
version = "1.0.0"

[[hook]]
event = "post-task"
command = "notify-send 'Effigy' 'Task {{task.name}} {{task.status}}'"
platform = "linux"

[[hook]]
event = "post-task"
command = "osascript -e 'display notification \"{{task.name}} {{task.status}}\"'"
platform = "macos"
```

### Example 3: S3 Cache Extension

```toml
# github.com/effigy/extension-cache-s3/effigy-extension.toml
[extension]
name = "cache-s3"
version = "1.0.0"

[config]
bucket = { required = true }
prefix = { default = "effigy-cache/" }
region = { default = "us-east-1" }

[[hook]]
event = "pre-cache-read"
command = "aws s3 cp s3://{{config.bucket}}/{{config.prefix}}{{cache.key}} {{cache.path}}"

[[hook]]
event = "post-cache-write"
command = "aws s3 cp {{cache.path}} s3://{{config.bucket}}/{{config.prefix}}{{cache.key}}"
```

## Core vs. Extension Boundary

| Feature | Core | Extension |
|---------|------|-----------|
| Task execution | ✅ | ❌ |
| DAG scheduling | ✅ | ❌ |
| File watching | ✅ | ❌ |
| Local caching | ✅ | ❌ |
| Language tasks | ❌ | ✅ |
| Cloud caching | ❌ | ✅ |
| Notifications | ❌ | ✅ |
| IDE protocol | ❌ | ✅ |
| CI integration | ❌ | ✅ |
| Telemetry | ❌ | ✅ |

## Open Questions

1. Should there be a central registry beyond GitHub?
2. How to handle extension versioning conflicts?
3. What's the governance model for `effigy/*` extensions?
4. Should extensions declare dependencies on other extensions?
5. How to test extensions in isolation?

## Success Criteria

- Extension can be created in < 30 minutes
- Extension can be shared via GitHub
- Extension API is stable for 1.x releases
- Core remains small, extensions add functionality
- Security model is clear and configurable

## Related Concepts

- Concept: Task Templates
- Concept: Lifecycle Hooks
- Concept: Extension Distribution
- Roadmap: Phase 3, Track 14

