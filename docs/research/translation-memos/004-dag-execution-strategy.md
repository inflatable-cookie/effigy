# Translation Memo 004: DAG Execution Strategy

Status: Draft
Memo: 004
Owner: Research
Last updated: 2026-03-07
Related track: Track 04 — DAG Execution and Scheduling

## 1) Effigy problem statement

Effigy has DAG execution for managed tasks, but needs validation:
- Is the current dependency model appropriate?
- Should cycle detection be explicit?
- What execution visualization would help users?
- Are there performance improvements from other DAG executors?

## 2) External evidence summary

From comparative analysis of Make, Bazel, and Dagger:

**Make**:
- Implicit DAG from rules
- Limited parallelism (`-j` slots)
- No cycle detection (will infinite loop)
- Simple but limited

**Bazel (Skyframe)**:
- Functional evaluation graph
- Fine-grained incremental evaluation
- Dynamic parallel scheduling
- Complex but correct

**Dagger**:
- Container operation chaining
- Parallel independent operations
- Implicit DAG (no cycles possible in chaining model)
- Container overhead but hermetic

**Common patterns**:
- Explicit dependencies are clearer than implicit
- Parallel execution is essential
- Cycle detection prevents hangs
- Visualization helps understanding

## 3) Recommendation

**Retain Effigy's current DAG model with incremental enhancements:**

1. **Keep explicit TOML dependencies** — Clear and simple
2. **Add cycle detection** — Fail fast with clear errors
3. **Add execution visualization** — Task graph display
4. **Maintain parallel execution** — Continue current approach

### Not recommended

- Bazel-style Skyframe: Too complex for Effigy's use case
- Dagger-style containers: Overhead not justified
- Make-style implicit rules: Causes confusion

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Simpler than Bazel | Less incremental optimization | Good enough for task runner |
| No container isolation | Less hermeticity | Document best practices |
| Explicit only | More verbose than implicit | Clarity > brevity |

## 5) What must be true before adoption

Already true:
- [x] DAG model works for task dependencies
- [x] Parallel execution implemented
- [x] Managed tasks execute correctly

To add:
- [ ] Cycle detection implemented
- [ ] Task graph visualization works
- [ ] Performance benchmarked

## 6) Required prototype or validation work

**Phase 1: Cycle detection**
- [ ] Implement cycle detection at task load time
- [ ] Clear error messages
- [ ] Test with circular dependencies

**Phase 2: Visualization**
- [ ] `effigy tasks --graph` command
- [ ] ASCII or mermaid output
- [ ] Show dependency relationships

**Phase 3: Validation**
- [ ] Benchmark parallel execution
- [ ] User testing of visualization
- [ ] Performance vs. Make comparison

## 7) Promotion target

- [x] `concept contract work` — Document DAG design
- [ ] `roadmap execution planning` — Cycle detection roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| Bazel dossier | high | Skyframe patterns |
| Dagger dossier | high | Container DAG |
| Make dossier | high | Baseline patterns |
| Track 04 synthesis | high | Cross-tool comparison |

## 9) Implementation plan

### Cycle detection

```rust
pub fn validate_dag(tasks: &TaskGraph) -> Result<(), DagError> {
    // DFS-based cycle detection
    // Clear error on cycle found
}
```

### Task graph visualization

```bash
$ effigy tasks --graph

build
├── deps
│   └── fetch
└── compile
    └── generate
```

### No changes to execution model

Current parallel execution is appropriate:
- Managed tasks run with TUI coordination
- Dependency order respected
- Resource limits via TUI slots

## Next Task

1. Create concept document: `docs/concepts/dag-execution.md`
2. Create roadmap for cycle detection implementation
3. Begin Track 05: Process Management and TUI Patterns

