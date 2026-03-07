# Track 13: IDE and Editor Integration

Status: Draft
Value track: IDE and Editor Integration (VS Code, cargo, LSP)
Created: 2026-03-07
Tools covered: VS Code Tasks, cargo IDE integration, Language Server Protocol

## 1) Synthesis

### Common Patterns

| Pattern | VS Code Tasks | cargo/rustc | LSP | Description |
|---------|--------------|-------------|-----|-------------|
| Configuration | tasks.json | Cargo.toml | Initialize params | Declarative or protocol-driven |
| Task discovery | Auto-detection + providers | Metadata commands | Server capabilities | Dynamic or static |
| Error parsing | Problem matchers | JSON diagnostics | Publish diagnostics | Output → IDE problems |
| Output format | Shell output | JSON/structured | Protocol messages | Human vs. machine |
| Extensibility | Extensions | Plugins | Any LSP client | Plugin architecture |

### Key Insights

**Two integration layers:**

| Layer | Purpose | Tools | Complexity |
|-------|---------|-------|------------|
| Task runner | Execute commands | VS Code Tasks, Makefile, npm | Low |
| Language server | Semantic understanding | rust-analyzer, LSP | High |

Effigy primarily operates at the task runner layer but can provide hooks for LSP integration.

**Machine-readable output is essential:**

IDEs need structured data:
- Error locations (file, line, column)
- Severity levels (error, warning, info)
- Error codes for documentation lookup
- Context for understanding

Human-readable output alone is insufficient.

**Auto-detection reduces friction:**

Best UX requires zero configuration:
- VS Code detects Makefile, package.json
- rust-analyzer detects Cargo.toml
- IDEs should detect effigy.toml

### What Works

**VS Code patterns:**
- tasks.json for custom tasks
- Problem matchers for error parsing
- Task providers for dynamic discovery
- Auto-detection for common cases

**cargo patterns:**
- JSON output (`--message-format=json`)
- Error codes with documentation
- Precise span information
- Metadata commands for introspection

**LSP patterns:**
- Separate process communication
- JSON-RPC protocol
- Publish diagnostics
- Code lenses for run/debug

### What Doesn't

**Anti-patterns:**
- Human-only output
- No error codes
- Imprecise locations
- Editor-specific formats

**Pain points:**
- Configuration duplication
- Format inconsistencies
- Manual task setup
- Limited introspection

## 2) Cross-Tool Capabilities Matrix

| Capability | VS Code Tasks | cargo | LSP | Effigy Should |
|------------|--------------|-------|-----|---------------|
| **Configuration** | tasks.json | Cargo.toml | Protocol | TOML (consistent) |
| **Task discovery** | Auto + providers | Metadata | Capabilities | Auto-detect + list |
| **Error format** | Problem matchers | JSON diagnostics | Protocol | JSON + human |
| **Output format** | Shell | JSON/text | JSON-RPC | Structured |
| **Locations** | Parsed | Precise spans | Range | File/line/col |
| **Error codes** | None | Systematic | Code | Error codes |
| **Extensibility** | Extensions | Plugins | LSP | Extensions |

## 3) Integration patterns

### Pattern 1: Auto-detection

IDE detects effigy.toml and suggests tasks:
```bash
# IDE runs:
effigy --list --format json

# Output:
[
  {"name": "build", "description": "Build the project"},
  {"name": "test", "description": "Run tests"},
  {"name": "lint", "description": "Run linter"}
]
```

### Pattern 2: JSON output

Machine-parseable output:
```bash
effigy build --format json
```

```json
{
  "task": "build",
  "status": "success",
  "outputs": [
    {
      "type": "diagnostic",
      "level": "error",
      "code": "E001",
      "message": "Command not found",
      "location": {
        "file": "effigy.toml",
        "line": 10,
        "column": 5
      }
    }
  ]
}
```

### Pattern 3: Problem matchers

VS Code pattern for Effigy output:
```json
{
  "problemMatcher": {
    "pattern": {
      "regexp": "^error\\[(E\\d+)\\]:\\s+(.*)$\\s+-->\\s+(.*):(\\d+):(\\d+)",
      "code": 1,
      "message": 2,
      "file": 3,
      "line": 4,
      "column": 5
    }
  }
}
```

### Pattern 4: VS Code extension

Task provider extension:
```typescript
vscode.tasks.registerTaskProvider('effigy', {
  provideTasks: async () => {
    const output = await exec('effigy --list --format json');
    const tasks = JSON.parse(output);
    return tasks.map(t => new vscode.Task(
      { type: 'effigy', task: t.name },
      vscode.TaskScope.Workspace,
      t.name,
      'effigy',
      new vscode.ShellExecution(`effigy ${t.name}`)
    ));
  }
});
```

## 4) Editor Comparison

| Editor | Task Integration | Extension API | Best For |
|--------|-----------------|---------------|----------|
| VS Code | tasks.json + providers | TypeScript | General purpose |
| JetBrains | Run configurations | Java/Kotlin | JVM languages |
| Vim/Neovim | Makefile + plugins | Lua/Vimscript | Keyboard-centric |
| Emacs | compile.el + plugins | Emacs Lisp | Extensibility |

Effigy should work with all via standard interfaces.

## 5) Gaps and Opportunities

### Gaps in current tools

1. **Configuration duplication**: tasks.json duplicates other configs
2. **Manual setup**: IDEs often require manual task configuration
3. **Limited introspection**: Hard to query available tasks
4. **Format inconsistencies**: Every tool has different output

### Opportunities for Effigy

1. **Auto-detection**: IDE detects effigy.toml automatically
2. **Task listing**: `effigy --list` for IDE consumption
3. **JSON output**: Structured output for error parsing
4. **Error codes**: Systematic error identification
5. **VS Code extension**: First-class integration
6. **LSP hooks**: Extension points for language servers

## 6) Recommendations for Effigy

### Core Principle

> Effigy should provide standard interfaces (JSON output, task listing) that any IDE can consume without custom configuration.

### Specific Recommendations

**1. Task Listing**
```bash
effigy --list              # Human readable
effigy --list --format json # Machine readable
```

Output:
```json
[
  {
    "name": "build",
    "description": "Build the project",
    "group": "build"
  }
]
```

**2. JSON Output**
```bash
effigy build --format json
```

```json
{
  "task": "build",
  "status": "success|failure",
  "duration_ms": 1234,
  "diagnostics": [
    {
      "level": "error|warning|info",
      "code": "E001",
      "message": "...",
      "location": {
        "file": "...",
        "line": 10,
        "column": 5
      }
    }
  ]
}
```

**3. Error Codes**
```
error[E001]: Task not found
  --> effigy.toml:15:5
   |
15 | task = "unknown"
   |       ^^^^^^^^^
   |
   = help: Run `effigy --list` to see available tasks
```

**4. VS Code Extension**
- Task provider for dynamic task discovery
- Problem matcher for Effigy output
- Configuration generation

**5. LSP Considerations**
- Extension points for language servers
- Configuration validation
- Task dependencies visualization

## 7) Open Questions

- Should Effigy provide its own language server?
- What level of IDE integration is "good enough" for MVP?
- How to handle task dependencies in IDE context?
- Should tasks have icons/descriptions for IDE display?

## 8) Next Steps

1. Design JSON output format
2. Implement `effigy --list --format json`
3. Define error code system
4. Create VS Code extension prototype
5. Document IDE integration patterns
6. Research Plugin Architecture (Track 14)
