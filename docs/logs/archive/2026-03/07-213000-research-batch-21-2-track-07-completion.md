# Research Batch 21.2: Track 07 Completion

Date: 2026-03-07
Roadmap: g01.021
Batch: 21.2

## Summary

Completed Batch 21.2 of Research Phase 2 (Developer Experience). Two tool dossiers and Track 07 value track synthesis completed.

## Deliverables

### Tool Dossiers (2)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [rustc](../../../research/tool-dossiers/rustc.md) | Complete | Industry-leading error format; visual, precise, helpful |
| [ESLint](../../../research/tool-dossiers/eslint.md) | Complete | Configurable rule-based system; auto-fixes |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 07: Error Reporting](../../../research/value-tracks/07-error-reporting-and-diagnostics.md) | Complete | Rustc-inspired format; clear, actionable, with error codes |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [007: Error Reporting Strategy](../../../research/translation-memos/007-error-reporting-strategy.md) | Draft | Implement rustc-style errors with error codes |

## Key Findings

### Error Format Comparison

| Tool | Style | Best For |
|------|-------|----------|
| rustc | Visual, detailed | Complex errors |
| ESLint | Rule-based, configurable | Validation |
| Go | Minimal | Simple cases |

### Recommended Error Format (Rustc-Inspired)

```
error[E001]: Task "build" failed
  --> effigy.toml:12
   |
12 | run = "cargo build --release"
   |       ^^^^^ command not found
   |
   = note: cargo is required for Rust projects
   = help: Install Rust: https://rustup.rs/
```

### Key Elements

1. **Error code** (E001): Searchable reference
2. **Clear message**: What went wrong
3. **Location**: File, line, column
4. **Context**: Surrounding code
5. **Underline**: Points to problem
6. **Notes**: Additional context
7. **Help**: Actionable suggestions

### Error Code Registry (Proposed)

```
E001: Task execution failed
E002: Configuration parse error
E003: Circular dependency detected
E004: Task not found
E005: Catalog not found
W001: Unused task (warning)
W002: Deprecated syntax (warning)
```

### Configurable Validation

```toml
[validation]
unused_tasks = "warn"
circular_deps = "error"
deprecated_syntax = "warn"
```

### Patterns to Adopt

- **Clear > clever**: Plain language
- **Location matters**: File, line, column
- **Visual formatting**: Colors, underlines
- **Actionable help**: Specific suggestions
- **Error codes**: Searchable references
- **JSON output**: For tooling integration

### Patterns to Reject

- **Too terse**: Go-style minimal (unhelpful)
- **Too verbose**: Wall of text
- **Inconsistent**: Different formats
- **No JSON mode**: Breaks automation

## Cumulative Research Progress

| Phase | Tracks Complete | Dossiers | Memos |
|-------|-----------------|----------|-------|
| Phase 1 | 5 | 12 | 5 |
| Phase 2 | 2 | 4 | 2 |
| **Total** | **7** | **16** | **7** |

## Next Batch

**Batch 21.3**: Track 08 — Monorepo Workspaces

Tools to study:
- Rush (workspace management)
- Nx (task graphs)

## Acceptance Criteria

- [x] 2 dossiers complete with source inventories
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] Error format defined with examples
- [x] Error code registry outlined

## Outcome

Batch 21.2 complete. Rustc-inspired error format recommended for Effigy: clear messages, precise locations, visual formatting, helpful suggestions, and error codes for reference. JSON output for tooling integration.

Ready to proceed to Batch 21.3 (Monorepo Workspaces).

