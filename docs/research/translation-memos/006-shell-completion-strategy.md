# Translation Memo 006: Shell Completion Strategy

Status: Draft
Memo: 006
Owner: Research
Last updated: 2026-03-07
Related track: Track 06 — Shell Completions

## 1) Effigy problem statement

Effigy has shell completions, but they need improvement:
- Currently basic and potentially stale
- No dynamic task name completion
- Hand-maintained completion scripts

## 2) External evidence summary

From comparative analysis of git, ripgrep, and cargo:

**git**:
- Hand-written, comprehensive, dynamic
- High maintenance burden
- Gold standard for UX

**ripgrep**:
- Generated with clap_complete
- Zero maintenance
- Static (no runtime context)

**cargo**:
- Hybrid: generated + some dynamic
- Moderate maintenance
- Task-aware (for cargo commands)

**Patterns**:
- Generated completions stay in sync
- Dynamic completions enable task awareness
- Multi-shell support expected
- Distribution should be easy

## 3) Recommendation

**Implement hybrid completion system:**

1. **Static completions**: Generate from CLI with clap_complete
2. **Dynamic tasks**: Runtime completion via `effigy completion tasks`
3. **Multi-shell**: bash, zsh, fish, PowerShell
4. **Easy distribution**: `effigy completion <shell>` command

### Implementation

```rust
// CLI definition with clap
#[derive(Parser)]
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
    /// List tasks (hidden, for completion)
    #[command(hide = true)]
    CompletionTasks,
}
```

```bash
# Installation
effigy completion bash > /etc/bash_completion.d/effigy
effigy completion zsh > /usr/share/zsh/site-functions/_effigy
effigy completion fish > ~/.config/fish/completions/effigy.fish
```

### Not recommended

- Hand-written scripts: Maintenance burden
- Pure static: Missing task names
- Runtime only: Too slow

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Hybrid complexity | More code | Well-structured, tested |
| Runtime parsing | Slower completion | Cache tasks, optimize parsing |
| Multi-shell support | More scripts | Generate most, custom minimal |

## 5) What must be true before adoption

- [x] clap supports completion generation
- [x] clap_complete is maintained
- [ ] Prototype: Task completion performance
- [ ] Test: Multi-shell compatibility

## 6) Required prototype or validation work

**Phase 1: clap_complete integration**
- [ ] Add clap_complete dependency
- [ ] Generate static completions
- [ ] Test in bash, zsh, fish

**Phase 2: Dynamic task completion**
- [ ] Implement `effigy completion tasks`
- [ ] Measure parsing performance
- [ ] Cache if needed

**Phase 3: Distribution**
- [ ] Document installation
- [ ] Test on different systems
- [ ] Consider package manager integration

## 7) Promotion target

- [x] `concept contract work` — Document completion design
- [ ] `roadmap execution planning` — Implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| git dossier | high | Dynamic patterns |
| ripgrep dossier | high | clap_complete patterns |
| Track 06 synthesis | high | Hybrid approach validated |

## 9) Implementation plan

### Phase 1: Static completions

```bash
$ effigy completion bash
# Generated bash script

$ effigy completion zsh
# Generated zsh script

$ effigy completion fish
# Generated fish script
```

### Phase 2: Dynamic tasks

```bash
$ effigy completion tasks
build
test
dev
lint
```

### Phase 3: Integration

```bash
# bash
complete -C 'effigy completion tasks' effigy

# Or in completion function
_effigy() {
    local tasks=$(effigy completion tasks 2>/dev/null)
    COMPREPLY=( $(compgen -W "$tasks" -- ${COMP_WORDS[COMP_CWORD]}) )
}
```

## Next Task

1. Create concept document: `docs/concepts/shell-completions.md`
2. Create implementation roadmap
3. Begin Track 07: Error Reporting

