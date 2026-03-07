# Translation Memo 007: Error Reporting Strategy

Status: Draft
Memo: 007
Owner: Research
Last updated: 2026-03-07
Related track: Track 07 — Error Reporting and Diagnostics

## 1) Effigy problem statement

Effigy needs to report errors clearly:
- Task execution failures
- Configuration problems
- Validation issues
- Dependency errors

Current error reporting needs improvement for clarity and actionability.

## 2) External evidence summary

From comparative analysis of rustc, ESLint, and others:

**rustc**:
- Visual formatting with underlines
- Precise locations (file, line, column)
- Helpful suggestions
- Error codes for reference
- Can be verbose

**ESLint**:
- Configurable severity
- Rule-based diagnostics
- Auto-fix suggestions
- Can be complex to configure

**Patterns**:
- Clear > clever
- Location matters
- Suggestions help
- JSON for tooling

## 3) Recommendation

**Implement rustc-inspired error format:**

### Format

```
error[E001]: Task "build" failed
  --> effigy.toml:12
   |
12 | run = "cargo build --release"
   |       ^^^^^ command not found
   |
   = note: cargo is required for Rust projects
   = help: Install Rust: https://rustup.rs/
   = help: Or use the container task: tasks.build.container
```

### Elements

1. **Error code** (E001): Searchable reference
2. **Message**: Clear description
3. **Location**: File, line, column
4. **Context**: Surrounding code
5. **Underline**: Points to problem
6. **Notes**: Additional context
7. **Help**: Actionable suggestions

### Configurable validation

For `effigy doctor`:

```toml
[validation]
unused_tasks = "warn"      # Warning only
circular_deps = "error"    # Blocks
deprecated_syntax = "warn" # Warning
```

### JSON output

```bash
effigy build --json
```

```json
{
  "status": "failure",
  "errors": [{
    "code": "E001",
    "severity": "error",
    "message": "Task 'build' failed",
    "location": {
      "file": "effigy.toml",
      "line": 12,
      "column": 7
    },
    "context": "run = \"cargo build --release\"",
    "help": "cargo is not installed"
  }]
}
```

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Verbosity | More output | Use --quiet for minimal |
| Implementation | More code | Structured error type |
| Maintenance | Error codes to manage | Document and test |

## 5) What must be true before adoption

- [x] Rust supports rich formatting
- [x] ANSI colors widely supported
- [ ] Prototype: Error format user testing
- [ ] Define: Error code registry

## 6) Required prototype or validation work

**Phase 1: Error type implementation**
- [ ] Create `Error` struct
- [ ] Implement Display trait
- [ ] Add color support

**Phase 2: Error codes**
- [ ] Define error code taxonomy
- [ ] Document common errors
- [ ] Add to --help

**Phase 3: Validation**
- [ ] User testing
- [ ] Compare to current format
- [ ] Iterate on feedback

## 7) Promotion target

- [x] `concept contract work` — Document error format
- [ ] `roadmap execution planning` — Implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| rustc dossier | high | Error format patterns |
| ESLint dossier | high | Configurable rules |
| Track 07 synthesis | high | Format validated |

## 9) Error Code Registry (Proposed)

```
E001: Task execution failed
E002: Configuration parse error
E003: Circular dependency detected
E004: Task not found
E005: Catalog not found
E010: Cache error
E011: Network error (remote cache)
W001: Unused task (warning)
W002: Deprecated syntax (warning)
```

## 10) Implementation plan

### Error struct

```rust
pub struct Error {
    pub code: ErrorCode,
    pub severity: Severity,
    pub message: String,
    pub location: Option<Location>,
    pub context: Option<String>,
    pub help: Vec<String>,
    pub notes: Vec<String>,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // rustc-style formatting
    }
}
```

### Display modes

```rust
pub enum OutputMode {
    Normal,  // With colors and formatting
    Plain,   // No colors, minimal formatting
    Json,    // JSON output
}
```

## Next Task

1. Create concept document: `docs/concepts/error-reporting.md`
2. Define error code registry
3. Begin Track 08: Monorepo Workspaces

