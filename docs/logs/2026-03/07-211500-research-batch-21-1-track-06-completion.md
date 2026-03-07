# Research Batch 21.1: Track 06 Completion (Phase 2 Start)

Date: 2026-03-07
Roadmap: g01.021
Batch: 21.1

## Summary

Completed Batch 21.1 of Research Phase 2 (Developer Experience). Two tool dossiers and Track 06 value track synthesis completed. **First batch of Phase 2 complete.**

## Deliverables

### Tool Dossiers (2)

| Dossier | Status | Key Findings |
|---------|--------|--------------|
| [git](../../research/tool-dossiers/git.md) | Complete | Gold standard for dynamic completions; hand-maintained scripts |
| [ripgrep](../../research/tool-dossiers/ripgrep.md) | Complete | Generated completions with clap_complete; zero maintenance |

### Value Track Synthesis (1)

| Track | Status | Recommendation |
|-------|--------|----------------|
| [Track 06: Shell Completions](../../research/value-tracks/06-shell-completions.md) | Complete | Hybrid: clap_complete for flags + dynamic for tasks |

### Translation Memo (1)

| Memo | Status | Action |
|------|--------|--------|
| [006: Shell Completion Strategy](../../research/translation-memos/006-shell-completion-strategy.md) | Draft | Implement clap_complete with dynamic task completion |

## Key Findings

### Completion Approaches Compared

| Tool | Approach | Dynamic | Maintenance |
|------|----------|---------|-------------|
| git | Hand-written | Yes | High |
| ripgrep | clap_complete | No | None |
| **Effigy (proposed)** | Hybrid | Yes | Low |

### Recommended Hybrid Approach

1. **Static flags**: Generate with clap_complete
   ```rust
   clap_complete::generate(shell, &mut app, "effigy", &mut stdout());
   ```

2. **Dynamic tasks**: Runtime via `effigy completion tasks`
   ```bash
   _effigy() {
       local tasks=$(effigy completion tasks)
       COMPREPLY=( $(compgen -W "$tasks" -- ${COMP_WORDS[COMP_CWORD]}) )
   }
   ```

3. **Distribution**: Built-in generation command
   ```bash
   effigy completion bash > /etc/bash_completion.d/effigy
   ```

### Patterns to Adopt

- **Generated completions**: Stay in sync with code
- **Dynamic task completion**: Essential for task runner
- **Multi-shell support**: bash, zsh, fish, PowerShell
- **Easy distribution**: Include in binary

### Patterns to Reject

- **Hand-maintained scripts**: Too error-prone
- **Pure static**: Missing core feature (task names)
- **Runtime only**: Too slow for flags

## Phase 2 Context

**Phase 2: Developer Experience** focuses on:
- 06: Shell Completions ✅ (just completed)
- 07: Error Reporting (next)
- 08: Monorepo Workspaces
- 09: Cross-Platform Portability
- 10: Environment Management

## Evidence Quality

| Source Type | Count | Confidence |
|-------------|-------|------------|
| Official documentation | 4 | high |
| Source code | 2 | high |
| clap_complete docs | 1 | high |
| Community usage | 2 | high |

## Cumulative Research Progress

| Phase | Tracks Complete | Dossiers | Memos |
|-------|-----------------|----------|-------|
| Phase 1 | 5 | 12 | 5 |
| Phase 2 | 1 | 2 | 1 |
| **Total** | **6** | **14** | **6** |

## Next Batch

**Batch 21.2**: Track 07 — Error Reporting and Diagnostics

Tools to study:
- Rustc (industry-leading error messages)
- ESLint (rule-based errors)

## Acceptance Criteria

- [x] 2 dossiers complete with source inventories
- [x] 1 value track synthesis with cross-tool comparison
- [x] 1 translation memo with actionable recommendation
- [x] Hybrid completion approach defined
- [x] clap_complete integration outlined

## Outcome

Batch 21.1 complete. Hybrid completion strategy validated: generated static completions (clap_complete) for flags + dynamic runtime completion for task names. This balances maintainability with essential functionality.

Ready to proceed to Batch 21.2 (Error Reporting).

