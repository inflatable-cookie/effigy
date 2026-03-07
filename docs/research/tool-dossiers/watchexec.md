# watchexec

Status: Draft
Tool name: watchexec
Category: file watcher (general-purpose)
Owner:
Last updated: 2026-03-07
Scope: watchexec 1.x/2.x documentation, library and CLI usage, cross-platform behavior

## 1) Why this tool matters

watchexec is a general-purpose file watcher and the underlying library for cargo-watch. It provides cross-platform file watching with debouncing, filtering, and signal handling. It's designed as both a standalone CLI tool and a Rust library.

For Effigy, watchexec represents:
- The most mature Rust file watching library
- Cross-platform file watching abstraction
- Both library and CLI patterns
- Debouncing and event handling best practices

## 2) Product and era context

### Timeline

- **2016**: Initial release by Félix Saparelli
- **2018**: v1.0 with cargo-watch adoption
- **2021**: v2.0 rewrite with improved API
- **2023-2024**: Continued refinement, better Windows support

### Design Philosophy

From the documentation:

> "Execute commands in response to file modifications"
> "A library and a CLI"

watchexec is explicitly dual-purpose:
1. Library for building custom watchers (like cargo-watch)
2. Standalone CLI for general use

### Target Audience

**Library users:**
- Tool builders (cargo-watch, etc.)
- Applications needing file watching

**CLI users:**
- Developers wanting language-agnostic watching
- CI/CD pipelines
- General automation

## 3) Defining architectural bets

### Cross-platform abstraction

watchexec normalizes platform differences:

| Platform | Backend |
|----------|---------|
| macOS | FSEvents |
| Linux | inotify |
| Windows | ReadDirectoryChangesW |
| Fallback | Polling |

Applications use one API; watchexec handles platform specifics.

### Event filtering

watchexec provides sophisticated filtering:

```bash
# Ignore patterns
watchexec --ignore '*.log' --ignore 'target/**' -- cargo build

# Watch specific extensions
watchexec --exts rs,toml -- cargo check

# Filter by path
watchexec --filter 'src/**' -- cargo test
```

### Debouncing with action

Events are debounced, but the action knows what changed:

```bash
watchexec -- echo "Changed: $WATCHEXEC_CHANGED_PATH"
```

Environment variables expose event details.

### Process lifecycle management

watchexec handles process management:
- SIGTERM/Ctrl-C handling
- Process restart on changes
- Clear screen between runs (optional)
- Restart signal configuration

```bash
watchexec --restart --clear -- cargo run
```

### Library-first design

watchexec is designed as a library first:

```rust
use watchexec::Watchexec;
use watchexec::config::Config;

let config = Config::default();
let wx = Watchexec::new(config, |action| {
    // Handle action
})?;
```

The CLI is a thin wrapper around the library.

## 4) Standout strengths

- **Cross-platform**: Works consistently on macOS, Linux, Windows
- **Multiple backends**: Uses best available native API
- **Debouncing**: Configurable delay to batch rapid changes
- **Rich filtering**: Path patterns, extensions, ignore lists
- **Process management**: Handles signal propagation, restart
- **Library API**: Can be embedded in other tools
- **Environment variables**: Exposes watch context to commands

## 5) Chronic weaknesses and recurring costs

### Complexity for simple cases

watchexec is powerful but can be verbose:

```bash
# Simple case is fine
watchexec -- cargo test

# Complex filtering gets long
watchexec --ignore 'target/**' --ignore '*.log' --exts rs,toml --filter 'src/**' -- cargo test
```

### Resource usage at scale

File watching consumes:
- File descriptors (one per watched file on some platforms)
- Kernel notification buffers
- CPU for event processing

Very large projects (10k+ files) can hit system limits.

### Platform behavior differences

While the API is unified, behavior varies:
- FSEvents (macOS) is coarse-grained (directory-level)
- inotify (Linux) is fine-grained (file-level)
- Windows can miss rapid changes

Applications must handle platform-specific quirks.

### Polling fallback performance

When native notifications unavailable:
- Polling consumes CPU
- Tradeoff between latency and CPU usage
- Not suitable for battery-powered devices

## 6) Between-release corrections

### v1 → v2 (2021)
- Complete API redesign
- Better error handling
- Improved Windows support
- Clearer separation of concerns

### v2.x evolution (2022-2024)
- Better documentation
- More examples
- Performance improvements
- Clearer environment variable contracts

The pattern: Maturation from "works" to "works well and documented."

## 7) Effigy-relevant lessons

### Adopt carefully

- **Library reuse**: Consider watchexec crate for Effigy's watch mode
- **Cross-platform abstraction**: Normalize platform differences
- **Debouncing**: Essential UX feature
- **Filtering**: Users need control over what triggers runs
- **Process lifecycle**: Handle signal propagation properly

### Reject early

- **Polling as primary**: Use native notifications, poll only as fallback
- **Complex configuration in CLI**: Config files > long CLI flags
- **Ignoring platform quirks**: Document and handle differences

### Prototype before deciding

- watchexec library integration in Effigy
- Resource usage on large Effigy-managed projects
- Comparison: native watchexec vs. Effigy's current watch

## 8) Comparison: watchexec vs. cargo-watch

| Aspect | watchexec | cargo-watch |
|--------|-----------|-------------|
| Scope | General-purpose | Cargo-specific |
| Configuration | CLI flags | CLI flags + Cargo smarts |
| Target | Any project | Rust projects |
| Library | Yes (primary) | Uses watchexec |
| Defaults | Minimal | Cargo-aware |

**Pattern**: Specialized tools (cargo-watch) built on general libraries (watchexec).

## 9) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [watchexec README](https://github.com/watchexec/watchexec) | official docs | current | high | Primary reference |
| [watchexec crates.io](https://crates.io/crates/watchexec) | metrics | current | high | Library usage |
| [API documentation](https://docs.rs/watchexec) | API docs | current | high | Library reference |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |
| Source code | source | current | high | Implementation |

## 10) Open questions

- How does watchexec handle filesystem events during the debounce window?
- What's the practical file count limit before resource issues?
- How well does the polling fallback work in practice?

## Next Task

Compare against cargo-watch and entr in Track 03 synthesis.

