# Research Batch 20.4: Track 04 Completion

Date: 2026-03-07
Roadmap: g01.020
Batch: 20.4

## Summary

Completed Batch 20.4 of Research Phase 1 (Core Execution). Bazel dossier deepened with Skyframe focus, Dagger dossier created, Track 04 value track synthesis completed.

## Deliverables

### Tool Dossiers (2)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [Bazel](../../research/tool-dossiers/bazel.md) | Deepened | Skyframe: functional evaluation graph, dynamic parallel scheduling, fine-grained incremental |
| [Dagger](../../research/tool-dossiers/dagger.md) | Complete | Container-based DAG, code-based pipelines, layer caching |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 04: DAG Execution](../../research/value-tracks/04-dag-execution-and-scheduling.md) | Complete | Keep current model, add cycle detection and visualization |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [004: DAG Execution Strategy](../../research/translation-memos/004-dag-execution-strategy.md) | Draft | Add cycle detection, task graph visualization |

## Key Findings

### DAG Model Comparison

| Tool | Model | Parallelism | Incremental |
|------|-------|-------------|-------------|
| Make | Implicit rules | `-j` slots | File timestamps |
| Bazel | Skyframe (functional) | Dynamic | Content-addressed |
| Dagger | Container chaining | Parallel ops | Layer caching |

### Effigy's Position

**Current model is appropriate:**
- Explicit TOML dependencies (clearer than Make's implicit)
- Parallel managed task execution (working well)
- Simpler than Bazel Skyframe (appropriate for task runner)

**Recommended enhancements:**
1. **Cycle detection**: Fail fast with clear errors
2. **Task graph visualization**: `effigy tasks --graph`

### Cycle Detection Algorithm

```rust
pub fn validate_dag(tasks: &TaskGraph) -> Result<(), DagError> {
    // DFS-based detection
    // Clear error on cycle found
}
```

### Task Graph Visualization

```bash
$ effigy tasks --graph

build
├── deps
│   └── fetch
└── compile
    └── generate
```

### Patterns to Adopt

- **Explicit dependencies**: Clearer than implicit rules
- **Parallel execution**: Essential for performance
- **Cycle detection**: Prevent infinite loops
- **Visualization**: Help users understand task relationships

### Patterns to Reject

- **Skyframe complexity**: Overkill for Effigy
- **Container overhead**: Not justified for task runner
- **Implicit rules**: Cause confusion (Make lesson)

## Evidence Quality

| Source Type | Count | Confidence |
|-------------|-------|------------|
| Official documentation | 6 | high |
| Academic/technical papers | 1 | medium |
| Source code | 2 | high |
| Community usage | 2 | medium |

## Next Batch

**Batch 20.5**: Track 05 — Process Management and TUI Patterns

Tools to study:
- cargo (output handling, progress)
- pnpm (concurrent output)

## Acceptance Criteria

- [x] Bazel dossier deepened with Skyframe details
- [x] Dagger dossier created
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] Cycle detection algorithm outlined
- [x] Visualization concept defined

## Outcome

Batch 20.4 complete. Effigy's current DAG model is validated as appropriate. Focus on incremental enhancements: cycle detection and task graph visualization. No major architectural changes needed.

Ready to proceed to Batch 20.5 (Process Management and TUI Patterns).

