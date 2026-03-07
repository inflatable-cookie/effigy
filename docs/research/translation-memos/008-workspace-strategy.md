# Translation Memo 008: Workspace Strategy

Status: Draft
Memo: 008
Owner: Research
Last updated: 2026-03-07
Related track: Track 08 — Monorepo Workspaces

## 1) Effigy problem statement

Effigy supports nested catalogs (workspaces) but could enhance:
- Change detection (run only what changed)
- Workspace visualization
- Cross-catalog dependencies
- Affected task execution

## 2) External evidence summary

From comparative analysis of Rush, Nx, and npm/pnpm:

**Rush**:
- Centralized configuration (rush.json)
- Strict dependency policies
- Git-based change detection
- Professional but rigid

**Nx**:
- Task graph visualization
- Computation caching
- Affected commands
- Interactive but JS-specific

**npm/pnpm**:
- Simple package.json workspaces
- Limited tooling
- Easy to adopt

**Patterns**:
- Change detection is valuable
- Visualization helps understanding
- Distributed vs. centralized tradeoffs
- Language-specific vs. agnostic

## 3) Recommendation

**Keep distributed catalogs, add change detection and visualization:**

### Current (keep)

```toml
# api/effigy.toml
[catalog]
alias = "api"

# core/effigy.toml
[catalog]
alias = "core"
```

### Add change detection

```bash
# Run tests in changed catalogs only
effigy test --changed-from origin/main

# Run tests in affected (changed + dependents)
effigy test --affected
```

### Add visualization

```bash
# Show catalog graph
effigy catalogs --graph

# Output formats:
# --format=ascii    # Terminal
# --format=mermaid  # Documentation
# --format=dot      # Graphviz
```

### Add cross-catalog dependencies

```toml
[catalog]
alias = "api"

[tasks.build]
depends = [
    { catalog = "core", task = "build" },
    { catalog = "database", task = "migrate" }
]
```

### Not recommended

- Centralized configuration: Too rigid
- Nx-style plugins: Language-specific
- Rush-style strictness: Overkill for most

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Distributed config | Harder to see big picture | Visualization helps |
| Git dependency | Requires git | Most projects use git |
| Implementation effort | Change detection complex | Start simple |

## 5) What must be true before adoption

- [x] Distributed catalogs work
- [ ] Change detection prototype
- [ ] Visualization prototype
- [ ] Performance on large repos

## 6) Required prototype or validation work

**Phase 1: Change detection**
- [ ] Git diff parsing
- [ ] Catalog change detection
- [ ] Affected graph computation

**Phase 2: Visualization**
- [ ] ASCII graph output
- [ ] Mermaid format
- [ ] Catalog relationship display

**Phase 3: Cross-catalog dependencies**
- [ ] Syntax for external deps
- [ ] Build order computation
- [ ] Validation

## 7) Promotion target

- [x] `concept contract work` — Document workspace design
- [ ] `roadmap execution planning` — Implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| Rush dossier | high | Strict policies |
| Nx dossier | high | Task graphs, caching |
| Track 08 synthesis | high | Distributed model validated |

## 9) Implementation plan

### Phase 1: Change detection

```rust
// Detect changed catalogs from git
pub fn changed_catalogs(base: &str) -> Vec<Catalog> {
    git::diff_names(base)
        .filter_map(|path| find_catalog_for_path(&path))
        .unique()
        .collect()
}

// Compute affected (changed + dependents)
pub fn affected_catalogs(changed: &[Catalog]) -> Vec<Catalog> {
    changed.iter()
        .flat_map(|c| all_dependents(c))
        .unique()
        .collect()
}
```

### Phase 2: Commands

```bash
# Changed only
effigy test --changed-from origin/main

# Affected (changed + dependents)
effigy test --affected --base=main

# Visualization
effigy catalogs --graph
effigy catalogs --graph --format=mermaid > docs/architecture.md
```

### Phase 3: Cross-catalog deps

```toml
[tasks.build]
depends = [
    { catalog = "core", task = "build" }
]
run = "cargo build"
```

## Next Task

1. Create concept document: `docs/concepts/workspace-strategy.md`
2. Begin Track 09: Cross-Platform Portability

