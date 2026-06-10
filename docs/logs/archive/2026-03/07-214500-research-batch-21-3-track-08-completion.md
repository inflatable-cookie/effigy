# Research Batch 21.3: Track 08 Completion

Date: 2026-03-07
Roadmap: g01.021
Batch: 21.3

## Summary

Completed Batch 21.3 of Research Phase 2 (Developer Experience). Two tool dossiers and Track 08 value track synthesis completed.

## Deliverables

### Tool Dossiers (2)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [Rush](../../../research/tool-dossiers/rush.md) | Complete | Centralized config, strict policies, change detection |
| [Nx](../../../research/tool-dossiers/nx.md) | Complete | Task graphs, visualization, affected commands |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 08: Monorepo Workspaces](../../../research/value-tracks/08-monorepo-workspaces.md) | Complete | Keep distributed, add change detection + visualization |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [008: Workspace Strategy](../../../research/translation-memos/008-workspace-strategy.md) | Draft | Add: change detection, catalog graph, cross-catalog deps |

## Key Findings

### Workspace Model Comparison

| Tool | Model | Change Detection | Visualization |
|------|-------|------------------|---------------|
| Rush | Centralized | Git-based | Limited |
| Nx | Graph-based | Affected commands | Interactive |
| **Effigy** | **Distributed** | **Add** | **Add** |

### Recommended Enhancements

1. **Change detection**
   ```bash
   effigy test --changed-from origin/main
   effigy test --affected
   ```

2. **Catalog visualization**
   ```bash
   effigy catalogs --graph
   effigy catalogs --graph --format=mermaid
   ```

3. **Cross-catalog dependencies**
   ```toml
   [tasks.build]
   depends = [
       { catalog = "core", task = "build" }
   ]
   ```

### Patterns to Adopt

- **Distributed catalogs**: Keep current model
- **Git-based change detection**: Standard approach
- **Task graph visualization**: Help users understand
- **Affected commands**: CI optimization

### Patterns to Reject

- **Centralized config**: Too rigid (Rush lesson)
- **Language-specific**: Stay agnostic
- **Over-strict policies**: Configurable instead

## Cumulative Research Progress

| Phase | Tracks Complete | Dossiers | Memos |
|-------|-----------------|----------|-------|
| Phase 1 | 5 | 12 | 5 |
| Phase 2 | 3 | 6 | 3 |
| **Total** | **8** | **18** | **8** |

## Phase 2 Remaining

| Batch | Track | Focus |
|-------|-------|-------|
| 21.4 | 09 | Cross-Platform Portability |
| 21.5 | 10 | Environment Management |

## Next Batch

**Batch 21.4**: Track 09 — Cross-Platform Portability

Tools to study:
- Just (cross-platform focus)
- Deno (modern portability)

## Acceptance Criteria

- [x] 2 dossiers complete with source inventories
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] Change detection approach defined
- [x] Visualization formats specified

## Outcome

Batch 21.3 complete. Keep Effigy's distributed catalog model, add:
1. Git-based change detection (`--changed-from`, `--affected`)
2. Catalog graph visualization (`--graph` with multiple formats)
3. Cross-catalog dependencies

Ready to proceed to Batch 21.4 (Cross-Platform Portability).
