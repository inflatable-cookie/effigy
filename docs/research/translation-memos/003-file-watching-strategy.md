# Translation Memo 003: File Watching Strategy

Status: Draft
Memo: 003
Owner: Research
Last updated: 2026-03-07
Related track: Track 03 — Watch Mode and File Monitoring

## 1) Effigy problem statement

Effigy has a `watch` command for running tasks on file changes. The current implementation needs validation against:
- Cross-platform file watching best practices
- Debouncing strategies
- Resource usage at scale
- Integration with Effigy's task model

## 2) External evidence summary

From comparative analysis of cargo-watch, watchexec, and entr:

**cargo-watch**:
- Debouncing (500ms default) is essential for UX
- Smart defaults (ignore target/, .git/) reduce noise
- Tool-specific integration provides value

**watchexec**:
- Library approach enables reuse across tools
- Cross-platform abstraction handles platform quirks
- Rich filtering is valuable for complex projects
- Process lifecycle management matters

**entr**:
- Simplicity has limits (no debouncing causes pain)
- Composability is powerful but requires user effort
- Unix-only limits applicability

**Common patterns**:
- Native OS notifications (FSEvents, inotify, ReadDirectoryChangesW)
- Debouncing to batch rapid changes
- Smart defaults for common ignores
- Clear user feedback on triggers

## 3) Recommendation

**Integrate the `watchexec` crate into Effigy's watch mode.**

### Rationale

1. **Proven code**: watchexec handles cross-platform complexity
2. **Active maintenance**: Regular updates, bug fixes
3. **Flexible API**: Supports Effigy's use case
4. **Rust-native**: Fits Effigy's ecosystem

### Implementation approach

```rust
// Effigy's watch command uses watchexec
use watchexec::Watchexec;
use watchexec::config::Config;

let config = Config::default()
    .path(watch_path)
    .filter(effigy_filter);  // Custom filtering

let wx = Watchexec::new(config, |action| {
    // Debounce and trigger Effigy task
})?;
```

### Configuration

```toml
[watch]
enabled = true
debounce = 500  # milliseconds

[watch.global]
ignore = [".git/", ".effigy/", "target/", "node_modules/"]

[tasks.dev.watch]
enabled = true
paths = ["src/**", "Cargo.toml"]
ignore = ["src/tests/**"]
```

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| Additional dependency | Larger binary | watchexec is ~100KB, worth it |
| Less control | Can't customize internals | watchexec is configurable |
| API stability | Dependency on external crate | watchexec is stable v2.x |

## 5) What must be true before adoption

- [x] watchexec crate is actively maintained
- [x] License compatible (Apache-2.0)
- [x] Cross-platform support verified
- [ ] Prototype integration tested
- [ ] Resource usage benchmarked

## 6) Required prototype or validation work

**Phase 1: Basic integration**
- [ ] Add watchexec dependency
- [ ] Implement basic watch mode
- [ ] Test on macOS, Linux, Windows

**Phase 2: Feature parity**
- [ ] Debouncing
- [ ] Smart ignores
- [ ] Clear output
- [ ] Process lifecycle management

**Phase 3: Comparison**
- [ ] Benchmark against current implementation
- [ ] Resource usage comparison
- [ ] User experience testing

## 7) Promotion target

- [ ] `concept contract work` — Document watch mode architecture
- [ ] `roadmap execution planning` — Create implementation roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| cargo-watch dossier | high | Debouncing essential |
| watchexec dossier | high | Library approach viable |
| entr dossier | high | Minimalism has limits |
| Track 03 synthesis | high | watchexec recommended |

## 9) Rejected alternatives

| Alternative | Reason for rejection |
|-------------|---------------------|
| Custom implementation | Must handle platform quirks, error-prone |
| Shell out to cargo-watch | External dependency, limited integration |
| entr-style minimalism | No debouncing is unacceptable UX |
| Polling only | Too resource-intensive |

## 10) Implementation plan (proposed)

### Phase 1: Dependency and basic integration

```toml
# Cargo.toml
[dependencies]
watchexec = "2.0"
```

### Phase 2: Effigy watch command

```bash
# Usage remains the same
effigy watch dev
effigy watch --once test
effigy watch --clear build
```

### Phase 3: Configuration

```toml
# effigy.toml
[watch]
debounce = 500
ignore = [".git/", "target/"]

[tasks.dev.watch]
enabled = true
paths = ["src/**"]
```

## Next Task

1. Prototype watchexec integration
2. Benchmark against current implementation
3. Begin Track 04: DAG Execution and Scheduling

