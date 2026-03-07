# Track 06: Shell Completions

Status: Draft
Track: Shell Completions
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `UX`, `CLI`

## 1) Problem statement

How should shell completions work? What balances:
- Completeness (cover all commands, flags, tasks)
- Accuracy (in sync with actual implementation)
- Performance (fast tab completion)
- Maintainability (don't require manual updates)

## 2) Why this track matters to Effigy

Effigy has completions, but should validate:
- Generation approach (static vs. dynamic)
- Multi-shell support
- Task name completion (dynamic)
- Installation and distribution

## 3) Cross-tool comparison

| Tool | Approach | Generation | Dynamic | Maintenance |
|------|----------|------------|---------|-------------|
| git | Hand-written scripts | Manual | Yes | High |
| ripgrep | clap_complete | Build-time | No | None |
| cargo | clap_complete + some dynamic | Hybrid | Partial | Low |
| Effigy (current) | Basic | Manual | No | Medium |

### Completion Spectrum

**Hand-written (git)**
- Pros: Context-aware, optimized
- Cons: High maintenance, error-prone

**Generated static (ripgrep)**
- Pros: Always accurate, zero maintenance
- Cons: No runtime context

**Hybrid (proposed for Effigy)**
- Static flags: Generated (clap_complete)
- Dynamic tasks: Runtime (parse effigy.toml)

## 4) Repeated patterns

### Universal completion requirements

1. **Flag completion**
   - Long flags (`--help`)
   - Short flags (`-h`)
   - Descriptions

2. **Command completion**
   - Subcommands
   - Hierarchy

3. **Argument completion**
   - File paths
   - Options
   - Custom (task names for Effigy)

### Shell differences

| Shell | System | Features |
|-------|--------|----------|
| bash | bash-completion | Basic |
| zsh | compsys | Advanced menus, descriptions |
| fish | Built-in | Syntax highlighting, descriptions |
| PowerShell | Register-ArgumentCompleter | Modern |

Each shell needs different completion script format.

## 5) Frontier research signals

- **Fig**: AI-powered completions (acquired by AWS)
- **Completion specs**: Declarative completion definitions
- **IDE-style completions**: LSP integration
- **Fuzzy matching**: More forgiving completion

## 6) Effigy implications

### Recommended direction

**Hybrid approach:**

1. **Static flags**: Generate with clap_complete
   ```rust
   // In build.rs
   clap_complete::generate_to(shell, &mut app, "effigy", outdir)?;
   ```

2. **Dynamic tasks**: Runtime completion function
   ```bash
   # Completion function calls effigy
   _effigy_tasks() {
       effigy completion tasks  # Returns task list
   }
   ```

3. **Distribution**: Include in binary
   ```bash
   effigy completion bash > /etc/bash_completion.d/effigy
   effigy completion zsh > /usr/share/zsh/site-functions/_effigy
   effigy completion fish > ~/.config/fish/completions/effigy.fish
   ```

### Risks to avoid

1. **Hand-maintained scripts**: Too error-prone
2. **No task completion**: Core feature missing
3. **Slow completion**: Parsing effigy.toml must be fast
4. **Stale completions**: Out of sync with code

### Evidence or prototype needed

- [ ] clap_complete integration
- [ ] Task completion performance
- [ ] Multi-shell testing
- [ ] Installation experience

## 7) Implementation suggestions

### Completion structure

```rust
// CLI definition
#[derive(Parser)]
#[command(name = "effigy")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate shell completions
    Completion {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// List tasks (for completion)
    #[command(hide = true)]
    CompletionTasks,
    // ... other commands
}
```

### Shell scripts

```bash
# bash completion
_effigy() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    
    # Static completions from generated file
    opts="--help --json --repo --version"
    
    # Dynamic task completion
    if [[ ${cur} != -* ]]; then
        opts+=$(effigy completion tasks 2>/dev/null)
    fi
    
    COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
}
complete -F _effigy effigy
```

### Performance considerations

- Cache task list (stale acceptable for completions)
- Parse only nearest effigy.toml
- Subsecond completion time

## 8) Comparison: Approaches

| Approach | Pros | Cons | Effigy |
|----------|------|------|--------|
| Hand-written | Context-aware | Maintenance burden | ❌ |
| clap_complete static | Zero maintenance | No dynamic tasks | Partial |
| clap + custom dynamic | Best of both | More complex | ✅ |
| Runtime only | Always current | Slower | ❌ |

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| git dossier | high | Hand-written patterns |
| ripgrep dossier | high | clap_complete patterns |
| clap_complete docs | high | Implementation |
| cargo completion | high | Hybrid approach |

## 10) Decision state

- [ ] `promote to concept work` — Document completion design
- [ ] `continue research` — Current approach needs improvement
- [ ] `prototype first` — Test clap_complete integration

**Current leaning**: Prototype first — integrate clap_complete with dynamic task completion.

## Next Task

1. Draft Translation Memo 006: Shell Completion Strategy
2. Begin Track 07: Error Reporting

