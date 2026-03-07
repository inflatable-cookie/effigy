# entr

Status: Draft
Tool name: entr (Event Notify Test Runner)
Category: file watcher (Unix philosophy)
Owner:
Last updated: 2026-03-07
Scope: entr 5.x documentation, Unix philosophy adherence, minimal design patterns

## 1) Why this tool matters

entr is a minimal file watcher that embodies Unix philosophy: do one thing well, compose with other tools. Created by Eric Radman, it's a single C file (~1000 lines) that watches files and runs commands when they change. It's the anti-Bazel: maximum simplicity, minimal features.

For Effigy, entr represents:
- Unix philosophy applied to file watching
- Minimal viable feature set
- Composability over integration
- The "small tool" end of the spectrum

## 2) Product and era context

### Timeline

- **2010**: Initial release by Eric Radman
- **2012-2024**: Steady maintenance, feature refinements
- **Current**: v5.x, mature and stable

### Design Philosophy

From the README:

> "A utility for running arbitrary commands when files change"
> "Uses kqueue(2) or inotify(7) to avoid polling"
> "Entirely separate from the command being run"

entr is explicitly minimal:
- No configuration file
- No debouncing options
- No complex filtering
- Just: watch these files, run this command

### Target Audience

- Unix/Linux users
- Developers who value simplicity
- Users of Make-based workflows
- Systems where minimal dependencies matter

### Cultural Context

entr is part of the "Unix philosophy" ecosystem:
- Like `find`, `xargs`, `grep`
- Composes via pipes
- Does one thing, does it well

## 3) Defining architectural bets

### Minimalism by design

entr has very few options:

```bash
# Watch specific files
ls *.rs | entr cargo test

# Watch directory recursively
find src -name '*.rs' | entr cargo build

# Restart server on changes
find src -name '*.rs' | entr -r cargo run

# Clear screen between runs
find src -name '*.rs' | entr -c cargo test
```

Options:
- `-r`: Restart (kill and restart on change)
- `-c`: Clear screen
- `-p`: Postpone (wait for first change)
- `-d`: Watch directories (for new files)

That's essentially it.

### Input via stdin

entr reads file list from stdin:
```bash
find . -name '*.rs' | entr cargo test
git ls-files '*.rs' | entr cargo check
ls src/*.rs | entr -r cargo run
```

This composes with any tool that lists files:
- `find`
- `git ls-files`
- `ls`
- `fd`
- Custom scripts

### No native recursion

entr watches specific files, not directories (by default):
```bash
# Must explicitly list files
find src -name '*.rs' | entr cargo test

# NOT:
# entr cargo test  (doesn't work)
```

This is intentional: explicit over implicit.

### Process replacement vs. management

With `-r` (restart), entr kills the running process and starts a new one. Simple and effective, but:
- No graceful shutdown handling
- No process groups
- SIGTERM, wait, then SIGKILL if needed

## 4) Standout strengths

- **Simplicity**: Single C file, no dependencies
- **Speed**: Native OS APIs (kqueue/inotify), no polling
- **Composability**: Works with any file-listing tool
- **Reliability**: Mature, stable, minimal bugs
- **Universality**: Works on any Unix (BSD, Linux, macOS)
- **Resource efficiency**: Minimal memory, few file descriptors

## 5) Chronic weaknesses and recurring costs

### Manual file listing

Users must explicitly specify what to watch:
```bash
# Requires find/git/ls
find src -name '*.rs' | entr cargo test

# vs. cargo-watch's automatic discovery
```

### No built-in filtering

Can't ignore files easily:
```bash
# Must filter before entr
find src -name '*.rs' ! -name '*_test.rs' | entr cargo test

# vs. watchexec's --ignore
```

### Limited restart control

`-r` is simple but crude:
- SIGTERM, wait, SIGKILL
- No graceful shutdown
- No signal selection

### No debouncing

Rapid file changes trigger multiple runs:
```bash
# If 10 files change, runs 10 times
# (unless tool batches, which entr doesn't)
```

This is particularly painful with `git checkout` or bulk edits.

### Unix-only

No Windows support (intentionally):
- Relies on kqueue (BSD/macOS) or inotify (Linux)
- No abstraction layer
- Port would require significant change

## 6) Between-release corrections

### v3 → v4 (2016)
- Directory watching with `-d` (for new files)
- Better signal handling

### v4 → v5 (2020)
- Improved inotify handling
- Better edge cases (symlinks, etc.)

The pattern: Incremental improvement while maintaining minimalism.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Explicit file lists**: Consider for debugging/composability
- **Unix philosophy**: Small tools that compose are maintainable
- **Simplicity**: Fewer features = fewer bugs
- **Native APIs**: Use kqueue/inotify directly when appropriate

### Reject early

- **No debouncing**: Essential for good UX
- **Manual file discovery**: Too tedious for daily use
- **Limited restart control**: Graceful shutdown matters
- **Unix-only**: Effigy targets Windows too

### Prototype before deciding

- Would entr-style composability help Effigy?
- How much does debouncing improve UX?
- What's the right balance: entr's minimalism vs. watchexec's features?

## 8) Comparison: entr vs. watchexec vs. cargo-watch

| Aspect | entr | watchexec | cargo-watch |
|--------|------|-----------|-------------|
| Philosophy | Unix minimalism | General-purpose library | Specialized tool |
| File discovery | External (stdin) | Built-in | Built-in + smarts |
| Debouncing | None | Configurable | Configurable |
| Filtering | External | Built-in | Limited |
| Platforms | Unix only | Cross-platform | Cross-platform |
| Complexity | Minimal (~1K LOC) | Moderate | Low (uses watchexec) |
| Best for | Scripts, minimalism | Applications, flexibility | Rust projects |

## 9) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [entr README](https://github.com/eradman/entr) | official docs | current | high | Primary reference |
| [entr source](https://github.com/eradman/entr/blob/master/entr.c) | source | current | high | Single C file |
| [Eric Radman's site](http://eradman.com/entrproject/) | official docs | current | high | Documentation |
| GitHub issues | community | ongoing | medium | Usage patterns |
| Unix community usage | observation | ongoing | medium | "Standard" minimal tool |

## 10) Open questions

- How do entr users handle bulk changes (git checkout, etc.)?
- What's the practical limit of entr's approach (scale)?
- How often is entr's composability actually used in complex ways?

## Next Task

Compare against watchexec and cargo-watch in Track 03 synthesis.

