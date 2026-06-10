# Research Batch 21.4: Track 09 Completion

Date: 2026-03-07
Roadmap: g01.021
Batch: 21.4

## Summary

Completed Batch 21.4 of Research Phase 2 (Developer Experience). Enhanced Just dossier with cross-platform focus, created Deno dossier, completed Track 09 value track synthesis.

## Deliverables

### Tool Dossiers (2)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [Just](../../../research/tool-dossiers/just.md) | Enhanced | Shell abstraction, `os()`/`arch()` functions, native Windows support |
| [Deno](../../../research/tool-dossiers/deno.md) | Complete | Single binary, Rust-based, permission model, cross-platform by design |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 09: Cross-Platform Portability](../../../research/value-tracks/09-cross-platform-portability.md) | Complete | Keep native shell, add platform conditionals, path helpers |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [009: Cross-Platform Strategy](../../../research/translation-memos/009-cross-platform-strategy.md) | Draft | Add platform conditionals, path normalization, Windows CI |

## Key Findings

### Cross-Platform Strategy Comparison

| Tool | Approach | Windows Support |
|------|----------|-----------------|
| Make | Minimal | Poor (MSYS required) |
| Just | Shell abstraction | Excellent |
| Deno | Single binary | Native |
| **Effigy** | **Native shell** | **Native (Rust)** |

### Recommended Approach

**Keep native shell with enhancements:**

1. **Native shell** (current)
   - Users write platform-specific commands
   - Maximum flexibility
   - No abstraction complexity

2. **Platform conditionals**
   ```toml
   [tasks.build]
   run = [
       { if = "windows", run = "build.bat" },
       { if = "unix", run = "./build.sh" }
   ]
   ```

3. **Path normalization**
   ```toml
   run = "compile {project}/src/main.rs"
   ```

4. **Windows CI testing**
   ```yaml
   os: [ubuntu, macos, windows]
   ```

### Patterns to Adopt

- **Single binary distribution**: Easy installation (already done)
- **Native shell**: Flexibility over abstraction
- **Platform conditionals**: For cross-platform tasks
- **Path helpers**: Automatic normalization
- **Comprehensive CI**: Test on all platforms

### Patterns to Reject

- **Shell abstraction**: Too complex (Just approach)
- **Ignoring Windows**: Must support all platforms
- **Assuming /bin/sh**: Windows users expect PowerShell

## Cumulative Research Progress

| Phase | Tracks Complete | Dossiers | Memos |
|-------|-----------------|----------|-------|
| Phase 1 | 5 | 12 | 5 |
| Phase 2 | 4 | 7 | 4 |
| **Total** | **9** | **19** | **9** |

## Phase 2 Remaining

| Batch | Track | Focus |
|-------|-------|-------|
| 21.5 | 10 | Environment Management (direnv, 1Password CLI) |

## Next Batch

**Batch 21.5**: Track 10 — Environment Management (final Phase 2 track)

Tools to study:
- direnv (directory-specific environment)
- 1Password CLI (secret injection)

## Acceptance Criteria

- [x] Just dossier enhanced with cross-platform focus
- [x] Deno dossier created
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] Platform conditional syntax proposed
- [x] Windows CI testing identified as essential

## Outcome

Batch 21.4 complete. Keep native shell approach for flexibility. Add platform conditionals for cross-platform tasks, path normalization helpers, and ensure Windows CI testing coverage.

Ready for final Phase 2 batch: Track 10 (Environment Management).

