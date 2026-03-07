# Bazel (Rules System)

Status: Draft
Tool name: Bazel
Category: Build system (rule/plugin architecture)
Owner:
Last updated: 2026-03-07
Scope: Bazel rules, rule functions, repository rules, toolchains

## 1) Why this tool matters

Bazel has a powerful extensible rules system. It's notable for:
- Starlark language for rule definitions
- Hermetic, reproducible builds
- Rich rule API for custom languages
- Repository rules for external dependencies

For Effigy, Bazel represents:
- Declarative rule definitions
- Build graph extensibility
- Hermetic execution model
- Language-agnostic plugin system

## 2) Product and era context

### Timeline

- **2015**: Bazel open-sourced (from Google Blaze)
- **2016**: Skylark (now Starlark) introduced
- **2017-2020**: Rules ecosystem growth
- **2020**: Bzlmod module system announced
- **2022-2024**: Bzlmod becomes default

### Design Philosophy

From Bazel documentation:

> "Rules are the heart of Bazel"
> "Rules produce outputs from inputs and dependencies"

### Target Audience

- Large monorepo users
- Multi-language projects
- Teams needing reproducible builds
- Custom language/tooling authors

### Ecosystem

- **rules_***: rules_go, rules_rust, rules_python, rules_docker
- **Bzlmod**: Module registry (registry.bazel.build)
- **Starlark**: Python-like rule language
- **Bazel Central Registry**: Official rule registry

## 3) Defining architectural bets

### Starlark rule definitions

Rules defined in Starlark (Python-like):

```python
# my_rule.bzl
def _my_rule_impl(ctx):
    # Access inputs
    input_file = ctx.file.src

    # Declare outputs
    output_file = ctx.actions.declare_file(ctx.attr.name + ".out")

    # Create action (build step)
    ctx.actions.run(
        outputs = [output_file],
        inputs = [input_file],
        executable = ctx.executable._tool,
        arguments = [input_file.path, output_file.path],
    )

    # Return providers (metadata for dependents)
    return [DefaultInfo(files = depset([output_file]))]

my_rule = rule(
    implementation = _my_rule_impl,
    attrs = {
        "src": attr.label(allow_single_file = True),
        "deps": attr.label_list(),
        "_tool": attr.label(
            default = "//tools:my_tool",
            executable = True,
            cfg = "exec",
        ),
    },
)
```

Benefits:
- Declarative
- Hermetic by default
- Rich metadata via providers

### Repository rules

Fetch external dependencies:

```python
# http_archive rule (built-in)
http_archive(
    name = "rules_rust",
    sha256 = "...",
    urls = ["https://github.com/bazelbuild/rules_rust/releases/download/..."],
)
```

Custom repository rule:
```python
def _git_repository_impl(ctx):
    # Clone git repo
    ctx.execute(["git", "clone", ctx.attr.remote, "."])
    ctx.execute(["git", "checkout", ctx.attr.commit])

git_repository = repository_rule(
    implementation = _git_repository_impl,
    attrs = {
        "remote": attr.string(),
        "commit": attr.string(),
    },
)
```

### Toolchains

Abstract over tools:

```python
# Define toolchain type
toolchain_type(name = "rust_toolchain_type")

# Register toolchain
rust_toolchain(
    name = "rust_linux",
    rustc = "@rust_linux//:rustc",
    cargo = "@rust_linux//:cargo",
)

toolchain(
    name = "rust_linux_toolchain",
    toolchain = ":rust_linux",
    toolchain_type = "@rules_rust//rust:toolchain_type",
    exec_compatible_with = ["@platforms//os:linux"],
)
```

Enables:
- Cross-compilation
- Remote execution
- Multiple tool versions

### Bzlmod module system

Modern dependency management:

```python
# MODULE.bazel
module(name = "my_project", version = "1.0.0")

bazel_dep(name = "rules_rust", version = "0.30.0")
bazel_dep(name = "rules_python", version = "0.28.0")
```

Replaces WORKSPACE-based dependency management.

## 4) Standout strengths

- **Hermetic builds**: Reproducible by design
- **Rich rule API**: Expressive build logic
- **Starlark**: Familiar, limited language
- **Toolchains**: Abstract tool dependencies
- **Repository rules**: Flexible external deps
- **Bzlmod**: Modern dependency management

## 5) Chronic weaknesses and recurring costs

### Steep learning curve

Starlark and Bazel concepts:
- Actions, providers, toolchains
- Execution platforms, target platforms
- Configuration transitions

Complex for simple use cases.

### Verbosity

Even simple rules require boilerplate:
```python
# Defining a rule requires:
# - Implementation function
# - Attribute schema
# - Provider returns
# - Toolchain declarations
```

### WORKSPACE to Bzlmod migration

Years-long migration:
- WORKSPACE deprecated
- Bzlmod still evolving
- Rules ecosystem catching up

### Slow startup

Loading and analysis:
- Starlark evaluation
- Repository fetching
- Dependency resolution

Can be slow for large projects.

## 6) Between-release corrections

### Early Bazel (2015-2017)
- Native rules only
- Limited extensibility

### Starlark era (2017-2020)
- Custom rules
- Repository rules
- Growing ecosystem

### Bzlmod era (2020-)
- Module-based dependencies
- Better version resolution
- Migration challenges

The pattern: Native → extensible → dependency management overhaul.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Declarative rules**: Clear input/output model
- **Hermetic execution**: Reproducible builds
- **Toolchains**: Abstract tool dependencies
- **Module system**: Dependency management

### Reject early

- **Complexity**: Too steep for task runner
- **Verbosity**: Keep plugin API simple
- **Starlark**: Don't invent new language
- **Migration pain**: Plan API stability

### Prototype before deciding

- Effigy plugin API complexity level
- Hermetic vs. practical tradeoffs
- Module/dependency system

## 8: Effigy Extension Architecture

### Option 1: Task templates

```toml
# effigy.toml
[extensions.node]
from = "github:effigy/extensions/node@v1"

[[task]]
template = "node:build"
name = "build"
entry = "src/index.js"
```

Extensions provide reusable task patterns.

### Option 2: Hook plugins

```toml
# effigy.toml
[extensions]
cache-s3 = { hook = "pre-cache", provider = "s3", bucket = "..." }
notify = { hook = "post-task", command = "notify-send" }
```

Lifecycle hooks for extending behavior.

### Option 3: WASM plugins

```rust
// Plugin interface
pub trait Plugin {
  fn register(&self, registry: &mut Registry);
}

// WASM module
#[no_mangle]
pub extern "C" fn register() {
  // Register tasks, hooks
}
```

Sandboxed, cross-platform plugins.

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Bazel docs](https://bazel.build/docs) | official docs | current | high | Primary reference |
| [Starlark language](https://bazel.build/rules/language) | docs | current | high | Rule language |
| [Rules tutorial](https://bazel.build/rules/rules-tutorial) | tutorial | current | high | Getting started |
| [Bzlmod](https://bazel.build/external/module) | docs | current | high | New module system |
| Bazel source | source | latest | high | Implementation |

## 10: Open questions

- What's the right complexity level for Effigy extensions?
- Should extensions be hermetic like Bazel?
- How to handle extension versioning?

## Next Task

Compare against ESLint and other plugin systems in Track 14 synthesis.

