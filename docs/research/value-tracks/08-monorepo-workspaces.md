# Track 08: Monorepo Discovery and Workspaces

Status: Draft
Track: Monorepo Discovery and Workspaces
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `SCALE`, `ARCH`, `UX`

## 1) Problem statement

How should monorepos be managed? What patterns help:
- Discover workspace structure
- Track dependencies between projects
- Run tasks selectively (only what changed)
- Visualize relationships

## 2) Why this track matters to Effigy

Effigy supports nested catalogs (workspaces). Research validates:
- Workspace discovery patterns
- Change detection approaches
- Task graph visualization
- Dependency tracking

## 3) Cross-tool comparison

| Tool | Workspace Model | Change Detection | Visualization | Caching |
|------|-----------------|------------------|---------------|---------|
| npm/pnpm | package.json workspaces | Limited | None | Basic |
| Rush | Centralized rush.json | Git-based | Limited | Yes |
| Nx | Inferred + config | Affected commands | Interactive graph | Computation |
| Effigy | Distributed effigy.toml | None yet | TUI | Basic |

### Workspace Model Spectrum

**Simple (npm/pnpm)**
- package.json workspaces array
- Minimal configuration
- Limited tooling

**Centralized (Rush)**
- rush.json defines all projects
- Strict policies
- Professional workflows

**Graph-based (Nx)**
- Task dependency graph
- Visualization
- Computation caching

**Distributed (Effigy)**
- effigy.toml in each catalog
- Flexible structure
- Nested discovery

## 4) Repeated patterns

### Universal workspace needs

1. **Discovery**
   - Find all projects/packages
   - Understand structure
   - Navigate relationships

2. **Dependencies**
   - Track inter-project dependencies
   - Build order
   - Avoid cycles

3. **Change detection**
   - What changed?
   - What depends on changes?
   - Run minimal set

4. **Task execution**
   - Run across projects
   - Parallelize
   - Report results

### Tool-specific innovations

**Rush: Strict policies**
- Consistent versions
- Approved packages
- Dependency validation

**Nx: Task graph + caching**
- Visualize dependencies
- Cache results
- Affected commands

**pnpm: Efficient installs**
- Content-addressable store
- Workspace filtering

## 5) Frontier research signals

- **Fine-grained change detection**: File-level dependencies
- **Distributed caching**: Share cache across CI/CD
- **Remote execution**: Run tasks on workers
- **IDE integration**: Workspace-aware editing

## 6) Effigy implications

### Recommended direction

**Enhance Effigy's workspace support:**

1. **Keep distributed catalogs** (current)
   - effigy.toml in each catalog
   - Natural project structure
   - No central config needed

2. **Add change detection**
   ```bash
   # Run tasks in changed catalogs
   effigy test --changed-from origin/main
   
   # Run tasks in affected (changed + dependents)
   effigy test --affected
   ```

3. **Add workspace visualization**
   ```bash
   # Show catalog graph
   effigy catalogs --graph
   
   # ASCII output
   # mermaid for docs
   # dot for Graphviz
   ```

4. **Enhanced dependency tracking**
   ```toml
   [tasks.build]
   # Explicit: depends on other catalogs
   depends = ["../core/build", "../utils/build"]
   ```

### Risks to avoid

1. **Centralized config requirement**: Keep distributed
2. **Language-specific assumptions**: Stay agnostic
3. **Over-complexity**: Simple defaults

### Evidence or prototype needed

- [ ] Change detection implementation
- [ ] Catalog graph visualization
- [ ] Performance on large workspaces

## 7) Implementation suggestions

### Change detection

```rust
fn get_changed_catalogs(base: &str) -> Vec<Catalog> {
    let changed_files = git::diff(base);
    changed_files
        .map(|f| find_catalog_containing(f))
        .unique()
        .collect()
}

fn get_affected_catalogs(changed: &[Catalog]) -> Vec<Catalog> {
    changed
        .iter()
        .flat_map(|c| c.dependents())
        .unique()
        .collect()
}
```

### Visualization

```bash
$ effigy catalogs --graph --format=mermaid

graph TD
    api --> core
    api --> database
    core --> utils
    web --> core
    web --> ui
```

### Enhanced dependencies

```toml
[catalog]
alias = "api"

[tasks.build]
# Depends on tasks in other catalogs
depends = [
    { catalog = "core", task = "build" },
    { catalog = "database", task = "migrate" }
]
run = "cargo build"
```

## 8) Comparison: Approaches

| Approach | Pros | Cons | Effigy |
|----------|------|------|--------|
| Centralized (Rush) | Consistent | Rigid | ❌ |
| Graph-based (Nx) | Visual | Complex | Partial |
| Distributed (Effigy) | Flexible | Less visible | ✅ Keep |

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| Rush dossier | high | Strict policies |
| Nx dossier | high | Task graphs |
| npm/pnpm docs | high | Simple workspaces |

## 10) Decision state

- [ ] `promote to concept work` — Document workspace design
- [ ] `continue research` — Sufficient guidance
- [ ] `prototype first` — Test change detection

**Current leaning**: Continue with distributed model, add change detection and visualization.

## Next Task

1. Draft Translation Memo 008: Workspace Strategy
2. Begin Track 09: Cross-Platform Portability

