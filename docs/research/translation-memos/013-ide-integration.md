# Translation Memo 013: IDE and Editor Integration

**Status:** Draft  
**Track:** 13 - IDE and Editor Integration  
**Tools:** VS Code Tasks, cargo IDE integration, Language Server Protocol  
**Date:** 2026-03-07  
**Related:** Translation Memo 007 (Error Reporting), Translation Memo 012 (CI/CD)

## Executive Summary

This memo translates Track 13 research findings into concrete implementation guidance for Effigy's IDE integration strategy. The key insight: **Effigy should provide standard, machine-readable interfaces (JSON output, task listing) that any IDE can consume without editor-specific configuration.**

## Research Summary

### VS Code Tasks
- **Strengths**: Flexible configuration, problem matchers, task providers, auto-detection
- **Weaknesses**: Configuration duplication, editor-specific, manual setup often required
- **Pattern**: tasks.json + extensions for dynamic task discovery

### cargo IDE Integration
- **Strengths**: JSON output, error codes, precise spans, LSP integration
- **Weaknesses**: Rust-specific, JSON verbosity
- **Pattern**: Machine-readable output + human-readable fallback

### Language Server Protocol
- **Strengths**: Editor-agnostic, rich features, standardized
- **Weaknesses**: Complex to implement, overkill for task running
- **Pattern**: Separate semantic layer from task execution

### Common Pattern
All successful IDE integrations provide:
1. Machine-parseable output (JSON)
2. Precise error locations
3. Task discovery mechanisms
4. Standard interfaces (not editor-specific)

## Core Principles

### 1. Editor-Agnostic Interfaces

Provide standard outputs any IDE can consume:

```bash
effigy --list --format json     # Task discovery
effigy build --format json      # Structured output
effigy validate --format json   # Configuration errors
```

### 2. Human + Machine Output

Same command, different formats:

```bash
effigy build              # Human readable (default)
effigy build --format json # Machine readable
```

### 3. Zero-Configuration Discovery

IDEs detect effigy.toml and automatically:
- List available tasks
- Provide problem matchers
- Suggest configurations

## Proposed Implementation

### Phase 1: Task Listing

**`effigy --list` command:**

```bash
# Human readable
effigy --list
# build    - Build the project
# test     - Run tests
# lint     - Run linter

# Machine readable
effigy --list --format json
```

```json
[
  {
    "name": "build",
    "description": "Build the project",
    "group": "build",
    "depends_on": ["install"]
  },
  {
    "name": "test",
    "description": "Run tests",
    "group": "test"
  }
]
```

### Phase 2: JSON Output

**Structured build output:**

```bash
effigy build --format json
```

```json
{
  "version": "1.0",
  "task": "build",
  "status": "success",
  "started_at": "2026-03-07T12:00:00Z",
  "duration_ms": 1234,
  "diagnostics": [
    {
      "level": "error",
      "code": "E001",
      "message": "Command not found: 'unknow-cmd'",
      "location": {
        "file": "effigy.toml",
        "line": 10,
        "column": 12,
        "end_column": 23
      },
      "context": {
        "line": "command = \"unknow-cmd\"",
        "highlight": "          ^^^^^^^^^^^"
      },
      "help": "Check the command name or install the missing tool"
    }
  ],
  "artifacts": [
    {
      "path": "target/release/myapp",
      "type": "executable"
    }
  ]
}
```

### Phase 3: Error Code System

**Error codes with documentation:**

```
error[E001]: Command not found
  --> effigy.toml:10:12
   |
10 | command = "unknow-cmd"
   |            ^^^^^^^^^^^
   |
   = help: Run `effigy explain E001` for more information
```

```bash
effigy explain E001
# Error E001: Command not found
#
# The specified command could not be found in PATH.
#
# Solutions:
# 1. Check the command spelling
# 2. Install the missing tool
# 3. Update PATH if the tool is installed in a non-standard location
```

### Phase 4: VS Code Extension

**Extension features:**

1. **Task Provider**: Auto-discover tasks from effigy.toml
2. **Problem Matcher**: Parse Effigy JSON output
3. **Configuration Generator**: Create .vscode/tasks.json
4. **Status Bar**: Quick task access

**package.json**:
```json
{
  "contributes": {
    "taskDefinitions": [
      {
        "type": "effigy",
        "required": ["task"],
        "properties": {
          "task": { "type": "string" }
        }
      }
    ],
    "problemMatchers": [
      {
        "name": "effigy",
        "pattern": {
          "regexp": "^error\\[(E\\d+)\\]:\\s+(.*)$",
          "code": 1,
          "message": 2
        }
      }
    ]
  }
}
```

### Phase 5: Editor Integrations

**Generic integration:**

| Editor | Integration Method |
|--------|-------------------|
| VS Code | Extension + tasks.json |
| JetBrains | External tool configuration |
| Vim/Neovim | Makeprg + errorformat |
| Emacs | Compile command |
| Sublime | Build system |

All use `effigy --list --format json` and `effigy --format json`.

### Phase 6: LSP Considerations (Future)

**Potential LSP features:**
- Configuration validation
- Autocompletion for task names
- Go to definition for task references
- Diagnostics for effigy.toml

Not required for MVP, but extension points should be considered.

## Implementation Priorities

| Priority | Feature | Rationale |
|----------|---------|-----------|
| P0 | `effigy --list --format json` | Task discovery |
| P0 | `effigy --format json` | Machine-readable output |
| P1 | Error code system | Documentation + IDE integration |
| P1 | VS Code extension | Primary IDE |
| P2 | Problem matchers | Error highlighting |
| P2 | Other editor docs | Vim, Emacs, JetBrains |
| P3 | LSP exploration | Future semantic features |

## JSON Schema

**Diagnostic schema:**

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Effigy Output",
  "type": "object",
  "required": ["version", "task", "status"],
  "properties": {
    "version": { "type": "string" },
    "task": { "type": "string" },
    "status": { "enum": ["success", "failure"] },
    "duration_ms": { "type": "integer" },
    "diagnostics": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["level", "message"],
        "properties": {
          "level": { "enum": ["error", "warning", "info"] },
          "code": { "type": "string" },
          "message": { "type": "string" },
          "location": {
            "type": "object",
            "properties": {
              "file": { "type": "string" },
              "line": { "type": "integer" },
              "column": { "type": "integer" },
              "end_column": { "type": "integer" }
            }
          }
        }
      }
    }
  }
}
```

## Open Questions

1. Should the JSON format be stable from v1.0?
2. What's the versioning strategy for the JSON schema?
3. Should tasks include icons for IDE display?
4. How to handle long-running tasks (watch mode) in IDEs?
5. Should task groups be standardized (build, test, etc.)?

## Success Criteria

- `effigy --list --format json` returns valid task list
- `effigy build --format json` returns structured output
- Error codes are documented with `effigy explain`
- VS Code extension can discover and run tasks
- No configuration required for basic IDE integration

## Related Concepts

- Concept: Machine-Readable Output
- Concept: Error Code System
- Concept: Editor Extension API
- Roadmap: Phase 3, Track 13

