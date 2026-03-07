# Translation Memo 005: TUI Patterns

Status: Draft
Memo: 005
Owner: Research
Last updated: 2026-03-07
Related track: Track 05 — Process Management and TUI Patterns

## 1) Effigy problem statement

Effigy has a TUI for managed tasks. Research validates:
- Is the multi-pane TUI the right approach?
- What output modes should be supported?
- How should concurrent process output be organized?
- What keyboard interaction patterns work best?

## 2) External evidence summary

From comparative analysis of cargo and pnpm:

**cargo**:
- Single progress bar, sequential output
- Progressive verbosity (-v, -vv)
- JSON output mode for tooling
- Excellent for focused tasks

**pnpm**:
- Concurrent output with prefixes
- Workspace-aware organization
- Stream to terminal
- Good for parallel tasks

**Common patterns**:
- Multiple output modes (interactive, plain, JSON)
- Progress indication is essential
- Concurrent output needs organization
- Keyboard control expected in interactive mode

## 3) Recommendation

**Retain Effigy's multi-pane TUI with refinements:**

1. **Keep TUI as default for interactive use**
   - Task list pane
   - Output pane
   - Keyboard navigation

2. **Support multiple output modes**
   ```bash
   effigy run        # TUI (default interactive)
   effigy run --plain  # Plain streaming (CI)
   effigy run --json   # Structured (tooling)
   ```

3. **Improve based on research**
   - Progressive disclosure (learn from cargo)
   - Output prefixing option (learn from pnpm)
   - Better ANSI handling

### Not recommended

- cargo-style sequential output: Too limited for concurrent tasks
- pnpm-style streaming: Less interactive than TUI
- Web-based TUI: Overkill for local tool

## 4) Tradeoffs Effigy accepts

| Tradeoff | Cost | Mitigation |
|----------|------|------------|
| TUI complexity | More code to maintain | ratatui library helps |
| Terminal requirement | Doesn't work in all contexts | --plain and --json modes |
| Learning curve | Users must learn shortcuts | Help overlay, intuitive keys |

## 5) What must be true before adoption

Already true:
- [x] TUI implementation exists and works
- [x] Multiple output modes supported
- [x] Keyboard navigation functional

To improve:
- [ ] Help overlay for shortcuts
- [ ] Better ANSI handling
- [ ] Performance benchmarking

## 6) Required prototype or validation work

**Phase 1: Improvements**
- [ ] Help overlay (press `?` for shortcuts)
- [ ] Output prefixing option (`--prefix`)
- [ ] Better ANSI emulation

**Phase 2: Validation**
- [ ] User testing of keyboard shortcuts
- [ ] Performance: TUI overhead measurement
- [ ] Accessibility review

## 7) Promotion target

- [x] `concept contract work` — Document TUI design
- [ ] `roadmap execution planning` — TUI improvement roadmap
- [ ] `watch only` — Not applicable
- [ ] `reject` — Not applicable

## 8) Sources

| Source | Confidence | Notes |
|--------|------------|-------|
| cargo dossier | high | Progressive output |
| pnpm dossier | high | Concurrent output |
| Track 05 synthesis | high | TUI patterns |

## 9) Implementation suggestions

### Help overlay

```
┌─────────────────────────────────────┐
│ Keyboard Shortcuts                  │
├─────────────────────────────────────┤
│ q        Quit                       │
│ r        Restart selected task      │
│ Tab      Switch focus between panes │
│ ↑/↓      Navigate tasks             │
│ Enter    Show task details          │
│ ?        Toggle this help           │
└─────────────────────────────────────┘
```

### Output prefixing option

```bash
# Plain mode with prefixes (like pnpm)
effigy run --plain --prefix

# Output:
# [build] Compiling...
# [test] Running tests...
# [build] Finished
```

## Next Task

1. Create concept document: `docs/concepts/tui-design.md`
2. Plan Phase 2 research (Developer Experience)
3. Summarize Phase 1 findings

