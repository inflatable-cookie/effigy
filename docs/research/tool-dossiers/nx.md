# Nx

Status: Draft
Tool name: Nx
Category: monorepo tool (task graphs, computation caching)
Owner:
Last updated: 2026-03-07
Scope: Nx 16.x/17.x documentation, task graph visualization, affected commands

## 1) Why this tool matters

Nx is a build system for monorepos with powerful features:
- Task graph visualization
- Computation caching
- "Affected" commands (change detection)
- Plugin ecosystem

Originally Angular-focused, now general-purpose for any JavaScript/TypeScript project.

For Effigy, Nx represents:
- Task graph visualization patterns
- Computation caching at scale
- Change detection workflows
- Plugin architecture

## 2) Product and era context

### Timeline

- **2017**: Nx created by Nrwl (now Nx)
- **2018-2020**: Angular-focused
- **2021**: General JavaScript/TypeScript support
- **2022-2024**: Major growth, independent company

### Design Philosophy

From Nx documentation:

> "Smart, Fast and Extensible Build System"
> "Never rebuild the same code twice"
> "Only run tasks affected by your changes"

### Target Audience

- JavaScript/TypeScript monorepos
- Teams wanting fast CI
- Developers wanting task visualization
- Organizations scaling their codebase

### Architecture Shift

Nx evolved from Angular CLI plugin to standalone build system:
- 2017-2020: Angular schematics, builders
- 2021+: General task runner
- Now: Language-agnostic plugins

## 3) Defining architectural bets

### Task graph visualization

Nx visualizes the task dependency graph:

```bash
nx graph
```

Opens interactive graph showing:
- Projects (libraries/apps)
- Dependencies between projects
- Task dependencies

Users can:
- Filter by project
- See critical path
- Understand relationships

### Computation caching

Nx caches task results:

```bash
nx build myapp  # First run: executes
nx build myapp  # Second run: from cache
```

Cache key based on:
- Source file hashes
- Dependencies
- Environment
- Configuration

### Affected commands

Nx detects changes and runs only affected tasks:

```bash
# Run test for changed projects and dependents
nx affected:test

# Build changed projects only
nx affected:build --base=main
```

Uses Git to detect file changes, then computes affected graph.

### Plugin architecture

Nx is extensible via plugins:

```bash
# Add Next.js support
npm install @nx/next

# Generate Next.js app
nx generate @nx/next:app myapp
```

Plugins provide:
- Generators (scaffolding)
- Executors (task runners)
- Migrations

### Project inference

Nx can infer projects from configuration:

```json
// nx.json
{
  "plugins": ["@nx/js"],
  "targetDefaults": {
    "build": {
      "inputs": ["{projectRoot}/**/*"],
      "outputs": ["{projectRoot}/dist"]
    }
  }
}
```

Reduces boilerplate configuration.

## 4) Standout strengths

- **Task graph visualization**: Interactive dependency graph
- **Computation caching**: Never rebuild same code
- **Affected commands**: Run only what's needed
- **Plugin ecosystem**: Extensible for any framework
- **CI optimization**: Distributed task execution
- **Editor integration**: VS Code extension

## 5) Chronic weaknesses and recurring costs

### Configuration complexity

Nx requires configuration:
- nx.json
- project.json (or package.json with nx config)
- workspace structure conventions

Learning curve for new users.

### JavaScript ecosystem lock-in

Nx is JS/TS focused:
- Plugins for JS frameworks
- Task runners assume npm/yarn/pnpm
- Less applicable to other ecosystems

### Migration effort

Adopting Nx in existing repo:
- Restructure workspace
- Configure projects
- Update CI/CD

Can be significant work.

### Commercial features

Some features require Nx Cloud (paid):
- Distributed caching
- Distributed task execution
- Analytics

## 6) Between-release corrections

### Nx 12-14 (2021-2022)
- Project crystal (inferred tasks)
- Better plugin API

### Nx 15-16 (2023)
- Improved task scheduling
- Better visualization

### Nx 17+ (2024)
- Standalone projects (simpler config)
- Improved caching

The pattern: Simplifying configuration while adding power.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Task graph visualization**: Help users understand dependencies
- **Affected/change detection**: Run only what's needed
- **Computation caching**: Don't repeat work
- **Plugin architecture**: Extensibility

### Reject early

- **Framework-specific plugins**: Effigy is language-agnostic
- **Project inference**: Explicit config is clearer
- **Complex configuration**: Keep simple defaults

### Prototype before deciding

- Task graph visualization for Effigy
- Change detection for CI optimization
- Computation caching integration

## 8) Comparison: Nx vs. Rush vs. Effigy

| Aspect | Nx | Rush | Effigy |
|--------|-----|------|--------|
| Focus | Task graphs, caching | Strictness, determinism | Task orchestration |
| Visualization | Interactive graph | Limited | TUI |
| Change detection | Affected commands | Git-based | Could add |
| Configuration | Moderate | Complex | Simple |
| Ecosystem | Plugins | Rush Stack | None needed |

**Pattern**: Nx emphasizes speed and visualization. Rush emphasizes strictness. Effigy could learn from both.

## 9) Effigy Enhancement Ideas

### Task graph visualization

```bash
# Show task dependencies as graph
effigy tasks --graph

# Output formats:
# --format=ascii   # Terminal ASCII
# --format=mermaid # For documentation
# --format=dot     # For Graphviz
```

### Affected tasks

```bash
# Run tasks affected by changes
effigy test --affected --base=main

# Build changed + dependents
effigy build --affected --include-dependents
```

### Enhanced caching

```toml
[cache]
enabled = true
# Content-addressable like Nx
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Nx docs](https://nx.dev/) | official docs | current | high | Primary reference |
| [Nx blog](https://nx.dev/blog) | blog | ongoing | high | Updates |
| GitHub issues/discussions | community | ongoing | medium | Usage |
| Nrwl conference talks | video | 2021-2024 | medium | Architecture |

## 11) Open questions

- How does Nx's caching scale to 1000+ projects?
- What's the performance overhead of task graph computation?
- How do teams handle Nx configuration drift?

## Next Task

Compare against Rush and other tools in Track 08 synthesis on monorepo patterns.

