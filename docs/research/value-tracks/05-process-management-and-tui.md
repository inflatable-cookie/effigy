# Track 05: Process Management and TUI Patterns

Status: Draft
Track: Process Management and TUI Patterns
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `UX`, `TUI`, `ARCH`

## 1) Problem statement

How should concurrent processes be displayed and managed? What TUI patterns balance:
- Visibility (see what's running)
- Clarity (not overwhelming)
- Interactivity (keyboard control)
- Performance (minimal overhead)

## 2) Why this track matters to Effigy

Effigy has a TUI for managed tasks. Research validates:
- Output organization patterns
- Progress indication
- Concurrent process display
- Keyboard interaction models

## 3) Cross-tool comparison

| Tool | Concurrency Model | Output Pattern | Interactivity |
|------|------------------|----------------|---------------|
| cargo | Buffered (one crate at a time) | Progress bar + compiler output | Limited (Ctrl-C only) |
| pnpm | Parallel with prefix | Prefixed lines by workspace | Limited (stream output) |
| Bazel | Parallel with event stream | Build event protocol | Query API |
| Effigy (current) | TUI with panes | Multi-pane terminal UI | Full keyboard control |

### TUI Pattern Spectrum

**Minimal (cargo)**
- Single progress bar
- Sequential output
- Simple and predictable

**Prefixed (pnpm)**
- Multiple concurrent outputs
- Prefix labels
- Stream to terminal

**Full TUI (Effigy)**
- Multiple panes
- Keyboard navigation
- Interactive controls
- Higher implementation cost

## 4) Repeated patterns

### Universal TUI requirements

1. **Progress indication**
   - Show what's running
   - Show completion status
   - Don't be noisy

2. **Output capture**
   - Preserve stdout/stderr
   - Allow inspection
   - Handle large output

3. **Error visibility**
   - Make failures obvious
   - Show relevant context
   - Allow drilling into details

4. **Control**
   - Start/stop/restart tasks
   - Keyboard shortcuts
   - Graceful shutdown

### Tool-specific innovations

**cargo: Progressive disclosure**
- Default: progress bar only
- `-v`: Show commands
- `-vv`: Show full output

**pnpm: Prefix organization**
```
[package-a] Building...
[package-b] Testing...
[package-a] Done
```

**Effigy (current): Multi-pane TUI**
- Task list pane
- Output pane
- Input/command pane
- Full keyboard navigation

## 5) Frontier research signals

- **Terminal multiplexer integration**: tmux/zellij-style panes
- **Web-based TUI**: Terminal in browser for remote dev
- **Structured logging**: JSON output for programmatic consumption
- **AI-assisted output**: Summarization, error explanation

## 6) Effigy implications

### Recommended direction

**Retain and refine Effigy's TUI approach:**

1. **Multi-pane TUI** (current)
   - Task list on the left
   - Output on the right
   - Footer with shortcuts

2. **Progressive output detail**
   ```bash
   effigy run        # TUI mode
   effigy run --plain  # Plain output (for CI)
   effigy run --json   # JSON output (for tooling)
   ```

3. **Keyboard shortcuts**
   - `q` quit
   - `r` restart selected task
   - `Tab` switch focus
   - Arrow keys navigate

4. **ANSI handling**
   - Preserve colors from child processes
   - Handle cursor movement
   - Terminal emulation for complex apps

### Risks to avoid

1. **Too much chrome**: Don't overwhelm with UI
2. **Broken pipe handling**: Child processes must be managed
3. **Resource leaks**: Clean up processes on exit
4. **Terminal state**: Restore on exit

### Evidence or prototype needed

- [x] Current TUI implementation works well
- [ ] Benchmark: TUI overhead vs. plain output
- [ ] User testing: Keyboard shortcut discoverability
- [ ] Validate: ANSI handling for complex apps

## 7) Implementation suggestions

### TUI Layout

```
┌─────────────────────────────────────────────────┐
│ [Effigy TUI - dev mode]                    [?] │
├──────────────┬──────────────────────────────────┤
│ Tasks        │ Output                           │
│ ○ build      │ Compiling main.rs...             │
│ ○ test       │ Finished dev [unoptimized]       │
│ ● dev        │ Running on http://localhost:3000 │
│ ○ lint       │                                  │
├──────────────┤                                  │
│ Shortcuts:   │                                  │
│ q=quit       │                                  │
│ r=restart    │                                  │
│ Tab=focus    │                                  │
└──────────────┴──────────────────────────────────┘
```

### Output modes

```bash
# TUI (default for interactive)
effigy run

# Plain (streaming, for CI)
effigy run --plain

# JSON (structured, for tooling)
effigy run --json
```

### Process lifecycle

```rust
enum TaskState {
    Pending,
    Running(Process),
    Completed(ExitStatus),
    Failed(ExitStatus, Output),
}

// Graceful shutdown
impl Drop for ManagedProcess {
    fn drop(&mut self) {
        self.send_signal(SIGTERM);
        // Wait with timeout
        // SIGKILL if necessary
    }
}
```

## 8) Comparison: Output mode tradeoffs

| Mode | Use Case | Pros | Cons |
|------|----------|------|------|
| TUI | Interactive dev | Rich UI, keyboard control | Requires terminal |
| Plain | CI/logging | Simple, pipeable | Less information density |
| JSON | Tooling integration | Machine-readable | Not human-friendly |

**Recommendation**: All three modes supported (Effigy already does this).

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| cargo dossier | high | Output patterns |
| pnpm dossier | high | Concurrent output |
| ratatui docs | official | TUI implementation |
| Effigy current TUI | observation | Working baseline |

## 10) Decision state

- [x] `promote to concept work` — Document TUI patterns
- [ ] `continue research` — Current approach sufficient
- [ ] `prototype first` — Refine based on user feedback

**Current leaning**: Effigy's TUI is appropriate. Focus on refinement, not redesign.

## Next Task

1. Draft Translation Memo 005: TUI Patterns
2. Complete Phase 1 research
3. Summarize findings for Phase 2 planning

