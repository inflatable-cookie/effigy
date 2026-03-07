# Track 03: Watch Mode and File Monitoring

Status: Draft
Track: Watch Mode and File Monitoring
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `UX`, `PERF`, `PORTABILITY`

## 1) Problem statement

How should file watching work? What balances:
- Responsiveness (detect changes quickly)
- Efficiency (don't consume excessive resources)
- UX (don't run commands on every keystroke)
- Cross-platform consistency

## 2) Why this track matters to Effigy

Effigy has a `watch` command, but should validate:
- Debouncing strategy (delay before running)
- Platform notification APIs (FSEvents, inotify, etc.)
- Resource usage at scale
- Integration with Effigy's task model

## 3) Cross-tool comparison

| Tool | Strategy | Strengths | Failure modes | Effigy signal |
|------|----------|-----------|---------------|---------------|
| cargo-watch | Native APIs + debounce | Cargo-native, smart defaults | Cargo-only, no config file | Debouncing essential |
| watchexec | Library abstraction + filtering | Cross-platform, flexible | Resource usage at scale | Library reuse viable |
| entr | Minimal native APIs | Simple, fast, composable | No debouncing, Unix-only | Simplicity has limits |

### Watching Strategy Spectrum

**Minimal (entr)**
- Explicit file listing via stdin
- No debouncing
- Native APIs only (kqueue/inotify)
- Unix only

**Balanced (cargo-watch)**
- Smart defaults (ignore target/, .git/)
- Configurable debounce
- Cross-platform via watchexec
- Tool-specific integration

**Maximum flexibility (watchexec)**
- Rich filtering (paths, extensions, ignore)
- Configurable debounce
- Process lifecycle management
- Library + CLI

## 4) Repeated patterns

### Universal requirements

1. **Native OS notifications**
   - macOS: FSEvents
   - Linux: inotify
   - Windows: ReadDirectoryChangesW
   - Polling only as fallback

2. **Debouncing**
   - Batch rapid changes (editor saves, git operations)
   - Typical delay: 100-500ms
   - Configurable per workflow

3. **Smart defaults**
   - Ignore build artifacts
   - Ignore version control (.git/)
   - Ignore dependency directories

4. **Clear feedback**
   - Show what triggered the run
   - Show what's being watched
   - Status indication (waiting, running, etc.)

### Platform differences

| Aspect | macOS (FSEvents) | Linux (inotify) | Windows |
|--------|------------------|-----------------|---------|
| Granularity | Directory-level | File-level | File-level |
| Latency | Higher | Lower | Medium |
| Resource usage | Lower | Higher | Medium |
| Reliability | Excellent | Good | Good |

Applications must handle coarse-grained events on macOS.

## 5) Frontier research signals

- **fanotify** (Linux): Newer alternative to inotify with different semantics
- **FSEvents improvements** (macOS): Better latency in recent versions
- **Polling optimizations**: Adaptive polling (faster when active, slower when idle)
- **Virtual filesystems**: WSL, Docker volume watching challenges

## 6) Effigy implications

### Recommended direction

**Built-in watch with watchexec library:**

1. **Use `watchexec` crate**
   - Proven cross-platform code
   - Regular maintenance
   - Handles platform quirks

2. **Smart defaults**
   ```toml
   [watch]
   debounce = 500  # ms
   ignore = [".git/", ".effigy/", "target/", "node_modules/"]
   ```

3. **Per-task watch configuration**
   ```toml
   [tasks.build.watch]
   enabled = true
   paths = ["src/**", "Cargo.toml"]
   debounce = 300
   ```

4. **Process lifecycle management**
   - Graceful shutdown (SIGTERM before SIGKILL)
   - Clear screen option
   - Restart on change vs. wait for completion

### Risks to avoid

1. **No debouncing**: Causes frustration with rapid changes
2. **Polling as primary**: Too resource-intensive
3. **Watching too much**: Build artifacts, dependencies, .git/
4. **Ignoring platform differences**: macOS FSEvents is coarse-grained

### Evidence or prototype needed

- [ ] Benchmark: watchexec integration vs. current implementation
- [ ] Test: Resource usage on large projects (10k+ files)
- [ ] Validate: Debounce delay that feels right (100ms? 500ms?)
- [ ] Verify: Cross-platform behavior consistency

## 7) Implementation suggestions

### Using watchexec library

```rust
use watchexec::Watchexec;
use watchexec::config::Config;
use watchexec::action::{Action, Outcome};

let config = Config::default()
    .path(".")
    .filter(|path| {
        // Custom filtering logic
        !path.starts_with("target/")
    });

let wx = Watchexec::new(config, |mut action| async move {
    // Debounce logic here
    // Run Effigy task
    action.outcome(Outcome::IfRunning(
        Interrupt,
        Start,
    ));
})?;
```

### Configuration design

```toml
[watch]
enabled = true
debounce = 500  # milliseconds

[watch.global]
ignore = [".git/", ".effigy/", "target/", "*.log"]

[tasks.dev.watch]
enabled = true
paths = ["src/**", "Cargo.toml"]
ignore = ["src/tests/**"]  # Additional ignores
```

### Commands

```bash
effigy watch dev          # Watch and run 'dev' task
effigy watch --once dev   # Run once on change, then exit
effigy watch --clear dev  # Clear screen between runs
```

## 8) Comparison: Build vs. Use watchexec

| Approach | Pros | Cons |
|----------|------|------|
| **Use watchexec crate** | Proven, maintained, cross-platform | Additional dependency |
| **Custom implementation** | Full control, no deps | Must handle platform quirks |
| **Shell out to cargo-watch** | Simple | Requires external tool, limited integration |

**Recommendation**: Use watchexec crate for reliability.

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| cargo-watch dossier | high | Debouncing patterns |
| watchexec dossier | high | Library API, cross-platform |
| entr dossier | high | Minimal approach limits |
| watchexec crate docs | high | Implementation reference |

## 10) Decision state

- [ ] `promote to concept work` — Document watch mode design
- [ ] `continue research` — Need prototype validation
- [ ] `prototype first` — Test watchexec integration

**Current leaning**: Prototype first — integrate watchexec crate and compare to current implementation.

## Next Task

1. Draft Translation Memo 003: File Watching Strategy
2. Begin Track 04: DAG Execution and Scheduling

