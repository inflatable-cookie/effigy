# Track 04: DAG Execution and Scheduling

Status: Draft
Track: DAG Execution and Scheduling
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `ARCH`, `PERF`, `SCALE`

## 1) Problem statement

How should task dependencies be represented and executed? What execution model balances:
- Correctness (dependencies run in order)
- Performance (parallel execution where possible)
- Understandability (users can reason about execution)
- Error handling (failures stop dependents, allow retries)

## 2) Why this track matters to Effigy

Effigy already has DAG execution for managed tasks. Research validates:
- Graph construction from task dependencies
- Parallel execution strategies
- Cycle detection
- Incremental evaluation patterns

## 3) Cross-tool comparison

| Tool | DAG Model | Parallelism | Caching | Use Case |
|------|-----------|-------------|---------|----------|
| Make | Implicit from rules | `-j` flag, job slots | Timestamps | Builds |
| Bazel | Skyframe (functional graph) | Dynamic scheduling | Content-addressable | Large builds |
| Dagger | Container dependency graph | Parallel containers | Layer caching | CI/CD |
| Airflow | Explicit task graph | Worker pools | XCom (task output) | Data pipelines |

### DAG Model Spectrum

**Implicit (Make)**
```makefile
test: build
    ./run-tests

build: compile
    ./link
```
- Pros: Simple, familiar
- Cons: Limited analysis, coarse parallelism

**Explicit functional (Bazel)**
```python
# SkyValue computed from dependencies
# Automatic parallelization of independent nodes
```
- Pros: Fine-grained, incremental, correct
- Cons: Complex implementation

**Container-based (Dagger)**
```go
// Container operations form DAG
// Parallel independent operations
```
- Pros: Hermetic, cacheable
- Cons: Container overhead

## 4) Repeated patterns

### Universal DAG requirements

1. **Dependency declaration**
   - Make: `target: dependency`
   - Bazel: Rule dependencies
   - Dagger: Operation chaining
   - Effigy: `task = [{ task = "dep" }]`

2. **Cycle detection**
   - Build graph, detect cycles before execution
   - Clear error messages (what depends on what)

3. **Parallel execution**
   - Identify independent nodes
   - Execute in parallel up to resource limits
   - Respect dependency order

4. **Failure handling**
   - Stop dependent tasks on failure
   - Allow independent tasks to continue (optional)
   - Report partial failures clearly

### Tool-specific innovations

**Bazel: Incremental evaluation**
- Only recompute changed nodes
- Content-addressed caching
- Fine-grained dependency tracking

**Dagger: Container-based isolation**
- Each operation in container
- Layer caching
- Reproducible execution

**Airflow: Dynamic task generation**
- Generate tasks at runtime
- Branching based on results
- Retry policies per task

## 5) Frontier research signals

- **Reactive DAGs**: Update graph as tasks run (dynamic dependencies)
- **Incremental computation**: Like Bazel but lighter weight
- **Serverless DAG execution**: Cloud functions as task executors
- **Streaming DAGs**: Process data streams through graph

## 6) Effigy implications

### Recommended direction

**Keep current DAG model, enhance with lessons learned:**

1. **Explicit dependency declaration** (already in Effigy)
   ```toml
   [tasks.build]
   run = [{ task = "deps" }, "cargo build"]
   ```

2. **Parallel execution** (already in Effigy for managed tasks)
   - Continue executing independent tasks in parallel
   - Respect TUI capacity limits

3. **Cycle detection** (should add)
   - Detect cycles at task load time
   - Clear error: "Task 'A' depends on 'B' which depends on 'A'"

4. **Execution visualization** (consider adding)
   - Show task graph: `effigy tasks --graph`
   - Visualize execution progress

### Risks to avoid

1. **Over-complexity**: Bazel's Skyframe is overkill for Effigy
2. **Container requirement**: Dagger's container overhead not justified
3. **Implicit dependencies**: Make's implicit rules cause confusion

### Evidence or prototype needed

- [ ] Cycle detection implementation
- [ ] Task graph visualization
- [ ] Performance: Parallel execution benchmarks
- [ ] UX: Execution progress clarity

## 7) Implementation suggestions

### Cycle detection

```rust
fn detect_cycles(tasks: &HashMap<String, Task>) -> Result<(), CycleError> {
    let mut visited = HashSet::new();
    let mut stack = HashSet::new();
    
    for task in tasks.keys() {
        if !visited.contains(task) {
            visit(task, &mut visited, &mut stack, tasks)?;
        }
    }
    Ok(())
}

fn visit(
    task: &str,
    visited: &mut HashSet<String>,
    stack: &mut HashSet<String>,
    tasks: &HashMap<String, Task>
) -> Result<(), CycleError> {
    stack.insert(task.to_string());
    
    for dep in tasks[task].dependencies() {
        if stack.contains(dep) {
            return Err(CycleError::new(task, dep));
        }
        if !visited.contains(dep) {
            visit(dep, visited, stack, tasks)?;
        }
    }
    
    stack.remove(task);
    visited.insert(task.to_string());
    Ok(())
}
```

### Execution visualization

```bash
# Show task dependency graph
effigy tasks --graph

# ASCII output:
# build
# ├── deps
# │   └── fetch
# └── compile
#     └── generate
```

## 8) Comparison: Effigy vs. alternatives

| Feature | Effigy | Bazel | Dagger |
|---------|--------|-------|--------|
| Dependency model | Explicit TOML | Skyframe | Container chaining |
| Parallelism | Yes (managed) | Yes (dynamic) | Yes (containers) |
| Cycle detection | Needed | Yes | Implicit (no cycles) |
| Caching | Basic | Advanced | Container layers |
| Complexity | Low | High | Medium |

**Effigy's position**: Simple, explicit DAG with room for enhancement.

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| Bazel Skyframe docs | official | high | Advanced DAG patterns |
| Dagger docs | official | high | Container DAG execution |
| Make manual | official | high | Baseline DAG |
| Airflow docs | official | medium | Workflow patterns |

## 10) Decision state

- [ ] `promote to concept work` — Document DAG design
- [ ] `continue research` — Current model sufficient
- [ ] `prototype first` — Cycle detection, visualization

**Current leaning**: Continue research is sufficient. Effigy's current DAG model is appropriate. Focus on enhancements: cycle detection, visualization.

## Next Task

1. Draft Translation Memo 004: DAG Execution
2. Begin Track 05: Process Management and TUI Patterns

