# ripgrep (rg)

Status: Draft
Tool name: ripgrep
Category: search tool (static completions, generated)
Owner:
Last updated: 2026-03-07
Scope: ripgrep completion generation, clap derive patterns, static completion distribution

## 1) Why this tool matters

ripgrep (rg) is a fast search tool with excellent CLI design. Its completions are:
- Generated from code (not hand-maintained)
- Distributed with the binary
- Complete and accurate
- Available for bash, zsh, fish, PowerShell

For Effigy, ripgrep represents:
- Generated completions pattern
- clap derive macro approach
- Distribution model
- Multi-shell support without hand-maintained scripts

## 2) Product and era context

### Timeline

- **2016**: ripgrep 0.1 released
- **2017**: clap integration for argument parsing
- **2018-2020**: Completion generation added
- **2021-2024**: Refinement, additional shell support

### Design Philosophy

From ripgrep's design:

> "Fast, correct, usable"
> "Sensible defaults"
> "Good citizen of the Unix ecosystem"

### Target Audience

- Developers searching code
- Tool builders (ripgrep as library)
- Users wanting fast grep alternative

### Implementation Stack

ripgrep uses:
- **clap**: Argument parsing with derive macros
- **clap_complete**: Completion script generation
- **Generated at build time**: No hand-maintained scripts

## 3) Defining architectural bets

### Generated completions (not hand-written)

ripgrep uses `clap_complete` to generate completions:

```rust
// In ripgrep's build.rs or CLI
use clap_complete::{generate, shells::{Bash, Zsh, Fish}};

let mut app = build_cli();
generate(Bash, &mut app, "rg", &mut io::stdout());
```

Benefits:
- Always in sync with code
- No maintenance burden
- Complete coverage

### Distributed with binary

ripgrep can generate completions on demand:

```bash
# Generate completions for your shell
rg --generate complete-bash
rg --generate complete-zsh
rg --generate complete-fish
```

Users install:
```bash
rg --generate complete-bash > /etc/bash_completion.d/rg
```

### Static completions

ripgrep's completions are static:
- Commands and flags don't change at runtime
- No dynamic suggestions based on filesystem
- Fast, no subprocess calls

Tradeoff: Less context-aware than git, but zero runtime cost.

## 4) Standout strengths

- **Always accurate**: Generated from code, never stale
- **Zero maintenance**: No hand-written completion scripts
- **Complete coverage**: Every flag, option, argument
- **Multi-shell**: bash, zsh, fish, PowerShell
- **Easy distribution**: Generate on demand

## 5) Chronic weaknesses and recurring costs

### Static limitations

ripgrep completions can't be context-aware:
```bash
# Can't suggest files that exist
rg --type <TAB>
# (shows all types, not just ones in current dir)

# Can't suggest from filesystem
rg <TAB>
# (no suggestions, unlike git which shows files)
```

### Build-time generation

Completions are generated at build time:
- Requires build script
- Adds to build complexity
- Must be kept in sync with releases

### Installation friction

Users must manually install completions:
```bash
# Not automatic
rg --generate complete-bash | sudo tee /etc/bash_completion.d/rg
```

## 6) Between-release corrections

### Early ripgrep (2016-2017)
- Basic completion scripts (hand-written)
- Limited shell support

### Modern ripgrep (2018-2024)
- clap integration
- Generated completions
- Multi-shell support

The pattern: Moved from hand-written to generated completions.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Generated completions**: Use clap_complete
- **On-demand generation**: `effigy completion bash`
- **Multi-shell**: Support bash, zsh, fish
- **Distribution**: Include in binary

### Reject early

- **Hand-maintained scripts**: Too error-prone
- **Build-time only**: Allow runtime generation

### Prototype before deciding

- clap_complete integration for Effigy
- Dynamic task completion alongside static flags
- Shell detection and installation

## 8) Comparison: ripgrep vs. git completions

| Aspect | ripgrep | git |
|--------|---------|-----|
| Generation | Build-time (clap) | Hand-maintained |
| Distribution | On-demand generation | Package manager |
| Dynamic | No | Yes |
| Maintenance | None | High |
| Completeness | Perfect | May lag features |

**For Effigy**: Hybrid approach—static flags (clap), dynamic tasks (runtime).

## 9) Effigy Implementation (Proposed)

### Using clap_complete

```rust
// CLI definition
#[derive(Parser)]
#[command(name = "effigy")]
struct Cli {
    #[arg(long)]
    json: bool,
    
    #[arg(short, long)]
    repo: Option<PathBuf>,
    
    // ... other flags
}

// Completion generation
fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "effigy", &mut stdout());
}
```

### Dynamic task completion

```rust
// Extend with dynamic completions
fn complete_tasks() -> Vec<String> {
    // Parse effigy.toml in current directory
    // Return task names
}
```

### Completion command

```bash
# Static completions (generated by clap)
effigy completion bash > /etc/bash_completion.d/effigy
effigy completion zsh > /usr/share/zsh/site-functions/_effigy
effigy completion fish > ~/.config/fish/completions/effigy.fish

# Dynamic task completion happens via shell function
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [ripgrep source](https://github.com/BurntSushi/ripgrep) | source | current | high | Implementation |
| [clap_complete docs](https://docs.rs/clap_complete) | official docs | current | high | Generation library |
| [ripgrep book](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md) | official docs | current | high | Usage patterns |

## 11) Open questions

- How to mix static (clap) and dynamic (task) completions?
- What's the latency of parsing effigy.toml for completions?
- Should completions be cached?

## Next Task

Compare against git and other tools in Track 06 synthesis on completion patterns.

