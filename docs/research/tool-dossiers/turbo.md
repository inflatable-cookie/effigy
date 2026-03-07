# Turbo (Turborepo)

Status: Draft
Tool name: Turbo / Turborepo
Category: build system (incremental builds, monorepo focus)
Owner:
Last updated: 2026-03-07
Scope: Turborepo 1.x/2.x documentation, remote caching, pipeline configuration

## 1) Why this tool matters

Turborepo (commonly called "Turbo") is Vercel's build system optimized for JavaScript/TypeScript monorepos. It has rapidly gained adoption in the JS ecosystem by offering Bazel-like caching with npm-like simplicity.

For Effigy, Turbo represents:
- A successful compromise between caching power and ease of use
- The "zero config" approach to build caching
- Remote caching as a service (commercial model)
- Pipeline/task graph execution model

## 2) Product and era context

### Timeline

- **2021**: Turborepo created by Jared Palmer
- **2021**: Open-sourced, rapid adoption in JS ecosystem
- **2021**: Vercel acquires Turborepo
- **2022**: Turborepo 1.0
- **2023**: Turborepo 1.10+ (improved watch mode, code generation)
- **2024**: Turborepo 2.0 (rewritten in Rust, 10x faster)

### Design Philosophy

From Turborepo documentation:

> "Your build system shouldn't require a PhD to use"
> "Never do the same work twice"
> "Cache locally, share remotely"

### Target Audience

- JavaScript/TypeScript monorepos
- Teams using npm/yarn/pnpm workspaces
- Developers who want caching without Bazel complexity
- Organizations willing to pay for remote caching

## 3) Defining architectural bets

### JSON configuration (not code)

Turbo uses `turbo.json` for configuration:

```json
{
  "pipeline": {
    "build": {
      "dependsOn": ["^build"],
      "outputs": [".next/**", "!.next/cache/**"]
    },
    "test": {
      "dependsOn": ["build"]
    }
  }
}
```

This is declarative, not programmable — simpler than Bazel's Starlark.

### Workspace-aware pipeline

Turbo understands monorepo structure:
- `^build` means "depend on `build` in workspace dependencies"
- Automatically discovers workspace graph from package manager
- No manual dependency declaration needed

### Task hashing

Turbo computes hashes based on:
- Task inputs (source files matching patterns)
- Environment variables (declared)
- Dependencies' outputs
- Task configuration

```
Hash = f(inputs, env, deps, config)
```

Unlike Bazel's full content hashing, Turbo uses configurable input patterns.

### Remote caching as a service

Vercel provides hosted remote caching:
```bash
turbo login
turbo link  # Connect to Vercel account
```

Or self-hosted with:
- Vercel's remote cache server
- Custom S3-compatible storage

### Rust rewrite (v2.0)

Turborepo 2.0 (2024) was rewritten from Go to Rust:
- 10x faster execution
- Better memory efficiency
- Native performance without Go's GC pauses

This validates Effigy's Rust choice.

## 4) Standout strengths

- **Zero config for simple cases**: Works with existing npm scripts
- **Automatic workspace discovery**: Understands pnpm/npm/yarn workspaces
- **Fast**: Rust implementation is very fast
- **Remote caching**: Easy setup with Vercel or self-hosted
- **Pipeline visualization**: See task graph with `turbo run --graph`
- **Watch mode**: Built-in file watching
- **IDE integration**: VS Code extension
- **Task filtering**: Run tasks only in specific workspaces

## 5) Chronic weaknesses and recurring costs

### JavaScript ecosystem lock-in

Turbo is optimized for JavaScript:
- Package.json script integration
- npm/yarn/pnpm workspace assumption
- Less valuable for non-JS projects

### Remote cache trust

Remote caching requires trusting the cache:
- CI and developers share cache
- Poisoned cache can cause confusing failures
- Cache versioning/salting needed for toolchain changes

### Environment variable tracking

Must explicitly declare environment variables that affect builds:
```json
{
  "pipeline": {
    "build": {
      "env": ["NODE_ENV", "API_URL"]
    }
  }
}
```

Missing an env var causes incorrect cache hits.

### Limited customizability

Compared to Bazel:
- No custom rules
- No query language
- Pipeline is the primary abstraction

## 6) Between-release corrections

### v1.0 → v1.10 (2022-2023)
- Improved watch mode stability
- Better error messages
- Code generation (gen) support

### v1.x → v2.0 (2024)
- **Rust rewrite**: Go → Rust for performance
- **New UI**: Terminal UI improvements
- **Improved hashing**: More accurate cache hits

The pattern: Turbo is investing in performance (Rust) while keeping simplicity.

## 7) Effigy-relevant lessons

### Adopt carefully

- **JSON/TOML configuration**: Declarative > programmable for task configs
- **Pipeline/task graph**: Visualizing dependencies helps users
- **Remote caching UX**: `login` + `link` pattern is smooth
- **Rust performance**: Validates Effigy's implementation choice
- **Watch mode**: Should be built-in, not external tool

### Reject early

- **JS ecosystem assumptions**: Effigy is language-agnostic
- **Implicit workspace discovery**: Explicit catalog definition is clearer
- **Vendor-hosted cache as primary**: Self-hosting should be first-class

### Prototype before deciding

- Turbo's task hashing vs. Effigy's current approach
- Remote cache API compatibility (could Turbo cache server work with Effigy?)
- Pipeline visualization for Effigy DAG

## 8) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Turborepo Docs](https://turbo.build/repo/docs) | official docs | 2.x | high | Primary reference |
| [Turbo 2.0 announcement](https://turbo.build/blog/turbo-2-0) | blog | 2024 | high | Rust rewrite |
| [Remote Caching](https://turbo.build/repo/docs/core-concepts/remote-caching) | official docs | 2.x | high | Cache implementation |
| [Pipeline docs](https://turbo.build/repo/docs/core-concepts/monorepos/running-tasks) | official docs | 2.x | high | Task execution |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |
| Vercel conference talks | video | 2022-2024 | medium | Architecture insights |

## 9) Open questions

- How does Turbo's Rust implementation compare to Effigy's architecture?
- Could Effigy users benefit from Turbo's remote cache server?
- What's the adoption rate of remote caching among Turbo users?

## Next Task

Compare against Bazel and sccache in Track 02 synthesis.

