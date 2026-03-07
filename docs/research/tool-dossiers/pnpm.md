# pnpm

Status: Draft
Tool name: pnpm
Category: package manager (concurrent execution, output handling)
Owner:
Last updated: 2026-03-07
Scope: pnpm 8.x/9.x documentation, concurrent script execution, output handling patterns

## 1) Why this tool matters

pnpm is a fast, disk-efficient package manager for Node.js. It's notable for:
- Content-addressable store (deduplication)
- Concurrent script execution (`pnpm run --parallel`)
- Excellent CLI output organization
- Workspace (monorepo) support

For Effigy, pnpm represents:
- Concurrent task execution patterns
- Output organization with multiple running processes
- Workspace/monorepo task management
- Modern package manager UX

## 2) Product and era context

### Timeline

- **2016**: pnpm created by Zoltan Kochan
- **2017**: Content-addressable store introduced
- **2020**: Workspace support added
- **2022**: v7 with improved concurrent execution
- **2023-2024**: v8/v9 with performance improvements

### Design Philosophy

From pnpm documentation:

> "Fast, disk space efficient package manager"
> "Strict package management"
> "Works reliably in monorepos"

### Target Audience

- Node.js developers
- Monorepo users
- Teams wanting fast, reliable package management
- CI/CD pipelines

### Differentiation

pnpm vs npm/yarn:
- **Content-addressable store**: Shared dependencies across projects
- **Strictness**: Only declared dependencies accessible
- **Performance**: Faster installs, better concurrency

## 3) Defining architectural bets

### Content-addressable store

pnpm stores packages in a content-addressed store:

```
~/.local/share/pnpm/store/
  v3/
    files/           # Content-addressed files
      00/1234abc...  # File content by hash
    index/           # Package metadata
```

Projects hardlink to the store, saving disk space.

### Concurrent script execution

pnpm can run scripts across workspaces concurrently:

```bash
# Run "build" in all workspaces
pnpm run --parallel build

# Run in filtered workspaces
pnpm --filter "./packages/*" run build
```

Output is organized by workspace:
```
packages/core build$ tsc
packages/utils build$ tsc
packages/core build: Done
packages/utils build: Done
```

### Output organization

pnpm organizes output from concurrent processes:
- Prefix with package name
- Collapse completed output
- Show only active output
- Optional per-package log files

### Workspace filtering

pnpm has powerful workspace filtering:

```bash
# Run in changed packages only
pnpm --filter "...[origin/main]" test

# Run in dependencies of a package
pnpm --filter "myapp..." build

# Run in dependents of a package
pnpm --filter "...mylib" test
```

This enables efficient monorepo workflows.

## 4) Standout strengths

- **Concurrent execution**: Run scripts across workspaces in parallel
- **Output organization**: Clear labeling of concurrent output
- **Disk efficiency**: Content-addressable store saves space
- **Performance**: Fast installs and script execution
- **Workspace filtering**: Powerful monorepo task targeting
- **Strictness**: Only declared dependencies accessible (catches bugs)

## 5) Chronic weaknesses and recurring costs

### Node.js ecosystem lock-in

pnpm is Node.js specific:
- Content-addressable store for npm packages
- Workspace model assumes package.json structure
- Scripts assumed to be npm scripts

### Concurrent output complexity

Managing output from many concurrent processes:
- Can be overwhelming with many workspaces
- Interleaved output if not careful
- Buffering tradeoffs (memory vs. responsiveness)

### Migration friction

From npm/yarn:
- Different lockfile format
- Different node_modules structure
- Some tools expect npm's flat node_modules

## 6) Between-release corrections

### v6 → v7 (2022)
- New lockfile format (v5.4)
- Improved concurrent execution
- Better workspace filtering

### v7 → v8 (2023)
- Performance improvements
- Better output organization
- Improved peer dependency handling

### v8 → v9 (2024)
- Further performance gains
- Continued workspace improvements

The pattern: Continuous performance and UX improvements.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Concurrent execution**: Run independent tasks in parallel
- **Output prefixing**: Label output by task/catalog
- **Filtering**: Allow running tasks in specific catalogs only
- **Workspace awareness**: Understand monorepo structure
- **Progress indication**: Show which tasks are active

### Reject early

- **Language-specific assumptions**: Effigy is language-agnostic
- **Store-based deduplication**: Not applicable to task runner
- **Strict dependency enforcement**: Different concern (build vs. run)

### Prototype before deciding

- pnpm-style concurrent output for Effigy's managed tasks
- Catalog filtering: `effigy test --catalog "packages/*"`
- Output prefixing in TUI: `[catalog-a/test] running...`

## 8) Comparison: pnpm vs. Effigy

| Aspect | pnpm | Effigy |
|--------|------|--------|
| Primary role | Package manager | Task runner |
| Concurrency | Parallel script execution | Parallel task execution |
| Output handling | Prefix with package name | TUI with panes |
| Workspace model | package.json workspaces | effigy.toml catalogs |
| Scope | Node.js only | Language-agnostic |

**Pattern**: pnpm handles concurrent npm scripts; Effigy handles concurrent tasks. Similar output challenges.

## 9) Effigy Integration Possibilities

### Option 1: Use pnpm for Node.js projects

```bash
# Effigy delegates to pnpm for Node.js tasks
effigy test
# → internally: pnpm run test
```

Pros: Leverages pnpm's workspace handling
Cons: Tight coupling to Node.js ecosystem

### Option 2: Learn from pnpm patterns

Adopt pnpm's output patterns in Effigy:
- Task name prefixing
- Concurrent output organization
- Catalog filtering

### Option 3: Complementary tools

Use both:
- pnpm for Node.js package management
- Effigy for orchestrating tasks (including pnpm commands)

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [pnpm docs](https://pnpm.io/) | official docs | current | high | Primary reference |
| [pnpm source](https://github.com/pnpm/pnpm) | source | current | high | Implementation |
| [pnpm blog](https://pnpm.io/blog) | blog | ongoing | high | Release notes |
| Workspace filtering docs | official docs | current | high | Monorepo patterns |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 11) Open questions

- How does pnpm handle output from 20+ concurrent processes?
- What's the memory overhead of output buffering?
- How does workspace filtering perform on large monorepos?

## Next Task

Compare against cargo and other tools in Track 05 synthesis on TUI patterns.

