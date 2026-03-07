# Track 01: Task Configuration Formats

Status: Complete
Track: Task Configuration and Manifest Formats
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `UX`, `CONFIG`

## 1) Problem statement

How should task definitions be structured and written? What format balances:
- Human writability and readability
- Tooling support (validation, completion, documentation)
- Learning curve (familiar vs. novel)
- Expressiveness (simple tasks vs. complex workflows)

## 2) Why this track matters to Effigy

Effigy currently uses TOML (`effigy.toml`). This choice should be validated against:
- What users are familiar with (Makefile, justfile, Taskfile.yml)
- What enables good error messages and IDE support
- What scales from simple tasks to complex workflows

## 3) Cross-tool comparison

| Tool | Format | Strengths | Failure modes | Effigy signal |
|------|--------|-----------|---------------|---------------|
| Make | Custom (tab-sensitive) | Ubiquitous, compact, incremental builds | Tab errors (`missing separator`), arcane syntax, poor error messages, portability | Avoid custom syntax; explicit help needed |
| Just | Custom (not tab-sensitive) | Modern, clear, good errors, built-in help | Yet another syntax to learn, no IDE support | Avoid custom syntax; `--list` pattern is right |
| Task | YAML | Familiar, structured, DAG execution, file tracking | Verbose, Go template complexity, indentation, performance | Structured format good; TOML > YAML |
| npm | JSON | Ubiquitous, simple | No comments, rigid, limited features, not a real task runner | JSON too limiting for config |
| Earthly | Earthfile (custom) | Container-native, repeatable builds | Custom syntax, niche adoption, learning curve | Custom syntax barrier to adoption |

### Detailed Format Analysis

**Make (Custom Syntax)**

```makefile
test: build
	cargo test
```

- **Tab sensitivity**: The most reported Make issue. Error `missing separator` doesn't explain the problem.
- **Help generation**: Requires copy-pasta boilerplate (~10 lines of shell)
- **String manipulation**: Functions like `$(patsubst)` require manual reading
- **Portability**: GNU vs BSD Make divergence causes CI failures

**Just (Custom Syntax)**

```justfile
test: build
    cargo test
```

- **No tab sensitivity**: Any whitespace works
- **Built-in help**: `just --list` uses comments automatically
- **Parameters**: `test filter=""`: — clear syntax
- **Cross-platform**: Shebang abstraction works on Windows

**Task (YAML)**

```yaml
tasks:
  test:
    deps: [build]
    cmds:
      - cargo test
```

- **Verbosity**: 5 lines vs 2 lines in Make/Just
- **Template complexity**: `{{.VAR}}` with Go template functions
- **DAG execution**: Native dependency graph support
- **File tracking**: Optional `sources`/`generates` with checksum or timestamp

### Format Categories

**Custom syntax (Make, Just)**
- Pros: Optimized for the specific use case
- Cons: Must learn new syntax, limited tooling

**Structured data (YAML, TOML, JSON)**
- Pros: Familiar, parser libraries exist, some tooling
- Cons: May not express certain patterns elegantly

**Embedded in code (cargo xtask, Invoke)**
- Pros: Full programming language power
- Cons: Must write code for simple tasks, barrier to entry

## 4) Repeated patterns

### Universal patterns (all tools)

1. **Tasks need names** — All formats have a task/recipe/target name as primary identifier
2. **Tasks need commands** — All have a way to specify what to run
3. **Default task** — `make`, `just`, `task` without arguments runs a default
4. **Help is essential** — Users need to discover available tasks

### Format-specific patterns

**Dependency declaration**:
- Make: `target: dependency`
- Just: `target: dependency` (recipe-level only)
- Task: `deps: [dependency]` (YAML list)
- Effigy: `task = [{ task = "dependency" }]` (TOML array)

**Parameter handling**:
- Make: Automatic (`$@`, `$<`, `$^`)
- Just: Explicit with defaults (`recipe arg="default":`)
- Task: Template variables with defaults
- Effigy: `{args}` interpolation for passthrough

**Help generation**:
- Make: Manual boilerplate required
- Just: Automatic from comments (`just --list`)
- Task: Automatic from task names
- Effigy: `effigy tasks` built-in

### Anti-patterns observed

1. **Tab sensitivity** (Make) — Causes silent, confusing errors
2. **Template complexity** (Task) — Go templates are powerful but esoteric
3. **Implicit rules** (Make) — Magic behavior that surprises users
4. **Whitespace significance** (YAML) — Indentation errors are common

## 5) Frontier research signals

- **Deno task**: JSON but with imports from URLs
- **Bun**: JavaScript-native, can import tasks
- ** mise (rtx)**: TOML-based, similar to Effigy's choice

## 6) Effigy implications

### Recommended direction

**TOML is the right choice for Effigy:**
1. Standard format (not custom syntax)
2. Better for human-written config than YAML (less indentation-sensitive, supports comments)
3. Rust ecosystem native (Effigy is Rust, users likely familiar)
4. Allows rich metadata without being verbose
5. Clear path to schema validation

### Risks to avoid

1. **Don't add templating complexity**: Keep interpolation simple (Effigy's `{var}` pattern is good)
2. **Don't let config become programming**: Task definitions should be declarative
3. **Don't sacrifice error message quality**: TOML parse errors should be user-friendly

### Evidence or prototype needed

- [ ] User testing: TOML vs YAML preference
- [ ] Validation: schema enforcement experience
- [ ] Migration: converting from Make/Just/Task to Effigy

## 7) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| Make manual | official | high | Baseline |
| Just documentation | official | high | Modern custom syntax |
| Task documentation | official | high | YAML approach |
| TOML spec | standard | high | Format choice |

## 8) Decision state

- [x] `promote to concept work` — TOML validated as right choice
- [x] `continue research` — Dossiers complete, sufficient for validation
- [ ] `prototype first` — Migration tooling can be deferred

**Decision**: TOML is validated as the correct choice for Effigy. The research confirms:
1. Custom syntax (Make, Just) creates learning barriers
2. YAML (Task) introduces verbosity and template complexity
3. TOML balances human writability with structure
4. Effigy's existing `{var}` interpolation is simpler than Go templates

## Next Task

Write Translation Memo 001 documenting the TOML recommendation. Begin Track 02 (Caching Strategies) with Bazel dossier.

