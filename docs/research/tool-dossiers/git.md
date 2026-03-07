# git

Status: Draft
Tool name: git
Category: version control (shell completions gold standard)
Owner:
Last updated: 2026-03-07
Scope: git completion system, dynamic completion generation, CLI UX patterns

## 1) Why this tool matters

git is the gold standard for shell completions. Its completion system is:
- Comprehensive (covers all commands, options, arguments)
- Dynamic (context-aware suggestions)
- Well-maintained across bash, zsh, fish
- Widely studied and copied

For Effigy, git represents:
- The benchmark for completion quality
- Dynamic completion patterns
- Complex argument handling
- Multi-shell support

## 2) Product and era context

### Timeline

- **2005**: git created by Linus Torvalds
- **2006-2010**: Basic completion scripts
- **2010-2015**: Dynamic completions added
- **2015-2024**: Continuous refinement

### Design Philosophy

git's CLI follows Unix conventions:
- Commands, subcommands, options, arguments
- Consistent flag patterns (`--long`, `-s` short)
- Help available everywhere (`git help <command>`)

### Completion Architecture

git uses different approaches per shell:

**bash**: Shell script (`git-completion.bash`)
- Sourced by users
- Dynamic via `__git_complete` functions
- Calls `git` for context

**zsh**: Native zsh completion system
- More sophisticated than bash
- Context-aware menus
- Better descriptions

**fish**: Built-in completions
- Generated from man pages
- Dynamic suggestions
- Syntax highlighting

## 3) Defining architectural bets

### Command hierarchy

git has a clear command structure:

```
git <command> [<subcommand>] [<options>] [<arguments>]

Examples:
git commit -m "message"
git remote add origin <url>
git branch -d feature-x
```

Completions understand this hierarchy and provide appropriate suggestions at each level.

### Dynamic completion generation

git completions are dynamic:

```bash
# Suggests only local branches
git checkout <TAB>

# Suggests remotes after "remote"
git remote <TAB>
# add  prune  remove  rename  set-head  set-branches...

# Suggests files after "add"
git add <TAB>
# (shows modified/untracked files)
```

This requires calling git to get current state:
```bash
git branch --list  # for branch completions
git remote         # for remote completions
```

### Completion descriptions

git completions include descriptions:

```bash
git --<TAB>
--help          show help
--version       show version
--exec-path     path to git programs
--html-path     path to HTML documentation
```

### Multi-shell support

git maintains separate completions for:
- bash (~800 lines)
- zsh (~300 lines, uses zsh's advanced features)
- fish (generated from man pages)

This is effort-intensive but provides native feel in each shell.

## 4) Standout strengths

- **Comprehensive**: Every command, option, argument covered
- **Dynamic**: Context-aware based on repo state
- **Descriptions**: Helpful text for each completion
- **Performance**: Fast even in large repos
- **Consistency**: Same patterns across all git commands
- **Documentation**: Completions reference official docs

## 5) Chronic weaknesses and recurring costs

### Maintenance burden

git completions are hand-maintained:
- New commands need completion updates
- Option changes require script changes
- Three shells = three scripts to maintain

### Complexity

git's completion scripts are complex:
- bash: ~800 lines of shell script
- zsh: sophisticated completion widgets
- Requires deep shell knowledge

### Distribution

Users must manually install completions:
```bash
# Often requires manual steps
cp git-completion.bash /etc/bash_completion.d/
```

Not all package managers set this up automatically.

## 6) Between-release corrections

### Early git (2005-2010)
- Basic command completion
- Limited option completion

### Modern git (2010-2024)
- Dynamic completions
- Better argument handling
- Improved performance

The pattern: Completions keep pace with git feature growth.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Command hierarchy**: Clear command/subcommand structure
- **Dynamic completions**: Context-aware suggestions
- **Descriptions**: Help text for each completion
- **Performance**: Fast completion generation

### Reject early

- **Hand-maintained scripts**: Too much effort
- **Multi-shell complexity**: Generate instead of hand-write
- **Manual distribution**: Include completions in binary

### Prototype before deciding

- Dynamic task name completion for `effigy <TAB>`
- Context-aware flag completion
- Performance of completion generation

## 8) Comparison: Static vs. Dynamic Completions

| Aspect | Static | Dynamic (git-style) |
|--------|--------|---------------------|
| Maintenance | Low (generated once) | High (hand-maintained) |
| Accuracy | May be stale | Always current |
| Context | Generic | Repo-specific |
| Performance | Fast | Requires computation |
| Examples | ripgrep, fd | git, cargo |

**Effigy's approach**: Dynamic for task names (from effigy.toml), static for flags.

## 9) Effigy Completion Design (Proposed)

### Static completions (built-in)

```bash
effigy --<TAB>
--help       show help
--json       output JSON
--repo       specify repo path
```

Generated at build time from CLI definition.

### Dynamic completions (runtime)

```bash
effigy <TAB>
# Queries effigy.toml for available tasks
build    # Build the project
test     # Run tests
dev      # Start dev server

# With prefix
effigy api/<TAB>
api/build
api/test
api/dev
```

### Implementation

```rust
// In completion handler
fn complete(shell: Shell) -> String {
    match shell {
        Shell::Zsh => generate_zsh_completions(),
        Shell::Bash => generate_bash_completions(),
        Shell::Fish => generate_fish_completions(),
    }
}

// Dynamic part: task discovery
fn complete_tasks() -> Vec<String> {
    // Parse effigy.toml
    // Return task names
}
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [git-completion.bash](https://github.com/git/git/blob/master/contrib/completion/git-completion.bash) | source | current | high | Bash completions |
| [git-completion.zsh](https://github.com/git/git/blob/master/contrib/completion/git-completion.zsh) | source | current | high | Zsh completions |
| [git documentation](https://git-scm.com/docs/git) | official docs | current | high | Command reference |
| Shell completion tutorials | blog | various | medium | Implementation patterns |

## 11) Open questions

- How does git maintain completion performance in large repos?
- What's the completion generation latency budget?
- How often do users actually use completions vs. muscle memory?

## Next Task

Compare against ripgrep and other tools in Track 06 synthesis on completion patterns.

