# cargo-watch

Status: Draft
Tool name: cargo-watch
Category: file watcher (Rust-specific)
Owner:
Last updated: 2026-03-07
Scope: cargo-watch 8.x/9.x documentation, usage patterns, integration with Cargo workflows

## 1) Why this tool matters

cargo-watch is the standard file watcher for Rust projects. It watches source files and runs Cargo commands on changes. Created by Félix Saparelli (passcod), it's the most popular Rust-specific watcher with tight Cargo integration.

For Effigy, cargo-watch represents:
- The Rust ecosystem's chosen file watching solution
- Cargo-centric workflow patterns
- A focused, single-purpose tool that does one thing well
- Integration patterns between watchers and build tools

## 2) Product and era context

### Timeline

- **2015**: Initial release
- **2018**: v7 rewrite with watchexec library
- **2020-2023**: Steady improvements, debouncing refinements
- **2024**: v9 with improved notification handling

### Design Philosophy

From the documentation:

> "Cargo Watch watches over your Cargo project's source"
> "It runs a command when a file changes"

### Target Audience

- Rust developers
- Users of Cargo workflows
- Developers wanting "save and run" development experience

### Key Positioning

cargo-watch is specifically for Cargo projects:
- Understands Cargo workspace structure
- Ignores `target/` directory automatically
- Integrates with Cargo's mental model

## 3) Defining architectural bets

### Cargo-centric integration

cargo-watch assumes a Cargo project:

```bash
# In a Cargo project directory
cargo watch -x test      # Run `cargo test` on changes
cargo watch -x "run --release"  # Run release build on changes
cargo watch -s "cargo build && ./target/debug/app"  # Custom shell
```

It automatically ignores:
- `target/` (build output)
- `.git/` (version control)
- Hidden files by default

### Built on watchexec

cargo-watch v7+ uses the `watchexec` library internally:
- Leverages battle-tested watching code
- Cross-platform notification handling
- Debouncing logic

This is an interesting pattern: specialized CLI on top of shared library.

### Debounced execution

Changes are debounced to avoid running commands on every keystroke:
- Default: 500ms delay
- Configurable: `--delay 1.5` for 1.5 seconds

### Multiple commands

cargo-watch can chain multiple Cargo commands:
```bash
cargo watch -x check -x test -x run
```

Runs `check`, then `test`, then `run` on each trigger.

## 4) Standout strengths

- **Cargo-native**: Understands Rust project structure
- **Smart defaults**: Ignores target/, .git/ automatically
- **Multiple backends**: Native OS notifications where available
- **Shell commands**: Not limited to cargo subcommands
- **Clear output**: Shows what's being watched and what triggered
- **Widely used**: Standard tool in Rust ecosystem

## 5) Chronic weaknesses and recurring costs

### Cargo-only focus

cargo-watch is tightly coupled to Cargo:
- Assumes `Cargo.toml` structure
- Optimized for Rust workflows
- Less useful for non-Rust projects

### No task configuration

Configuration is entirely command-line:
```bash
cargo watch -x test --ignore docs/
```

No config file for persistent watch settings (by design, but limits complexity).

### Single-project watching

Primarily designed for single Cargo projects:
- Can watch workspaces, but limited control
- No sophisticated multi-root watching
- No task-specific watching (watch these files for this task, those for that)

### Resource usage

File watchers consume resources:
- File descriptors for each watched file
- CPU for notification processing
- Memory for tracking state

On very large projects, this can be noticeable.

## 6) Between-release corrections

### v7 rewrite (2018)
- Switched to watchexec library
- Improved cross-platform support
- Better debouncing

### v8-v9 (2020-2024)
- Improved Windows support
- Better handling of large projects
- Clearer output formatting

The pattern: Stability and refinement rather than major feature additions.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Smart defaults**: Ignore build artifacts automatically
- **Debouncing**: Essential for UX (don't run on every keystroke)
- **Clear feedback**: Show what triggered the run
- **Multiple commands**: Support command chains
- **Library reuse**: watchexec pattern is worth considering

### Reject early

- **Tool-specific focus**: Effigy is language-agnostic
- **No configuration**: Persistent watch configs are useful
- **Single-project assumption**: Effigy supports nested catalogs

### Prototype before deciding

- Integration: Can cargo-watch trigger effigy tasks?
- Debouncing: What delay feels right for different workflows?
- Resource usage: How does Effigy's watch scale to large repos?

## 8) Effigy Integration Possibilities

### Option 1: Use cargo-watch as backend

```bash
# cargo-watch triggers effigy
cargo watch -s "effigy test"
```

Pros: Leverages existing tool
Cons: Two tools to install, cargo-centric

### Option 2: Native watch mode (current approach)

Effigy's built-in `watch` command:
```bash
effigy watch --once test
effigy watch --owner effigy --once test
```

Pros: Integrated experience
Cons: Need to maintain watch logic

### Option 3: watchexec library integration

Use the `watchexec` crate in Effigy:
```rust
// In Effigy's watch implementation
use watchexec::Watchexec;
```

Pros: Same proven code as cargo-watch
Cons: Additional dependency

## 9) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [cargo-watch README](https://github.com/watchexec/cargo-watch) | official docs | current | high | Primary reference |
| [cargo-watch on crates.io](https://crates.io/crates/cargo-watch) | metrics | current | high | Download stats |
| [watchexec library](https://github.com/watchexec/watchexec) | source | current | high | Underlying library |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |
| Rust community usage | observation | ongoing | high | "Standard" tool |

## 10) Open questions

- How does cargo-watch handle git branch switches (many file changes at once)?
- What's the resource usage on very large Rust projects (10k+ files)?
- Do users prefer `cargo watch` or IDE-based file watching?

## Next Task

Compare against watchexec and entr in Track 03 synthesis on file watching patterns.

