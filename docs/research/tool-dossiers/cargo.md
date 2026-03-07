# cargo (Rust package manager)

Status: Draft
Tool name: cargo
Category: package manager / build tool (TUI patterns)
Owner:
Last updated: 2026-03-07
Scope: cargo output handling, progress indication, error formatting, TUI patterns

## 1) Why this tool matters

cargo is Rust's build tool and package manager. It's widely praised for its excellent CLI user experience — clear output, helpful error messages, and polished progress indication. As a Rust tool itself, cargo represents patterns that Effigy (also Rust) should study carefully.

For Effigy, cargo represents:
- Industry-leading CLI/TUI patterns
- Rust-specific best practices
- Error message formatting excellence
- Progress indication without noise

## 2) Product and era context

### Timeline

- **2014**: cargo introduced with Rust 1.0
- **2015-2020**: Steady UX improvements
- **2021**: Improved progress bars, JSON message format
- **2022-2024**: Continued refinement, diagnostics improvements

### Design Philosophy

From cargo's design and Rust community feedback:

> "Good error messages are worth their weight in gold"
> "Progress should be visible but not noisy"
> "Defaults should work for most users"

### Target Audience

- Rust developers (primary)
- Tooling authors (cargo as library patterns)
- CLI designers (cargo as reference implementation)

## 3) Defining architectural bets

### Progressive output detail

cargo adapts output to context:

```bash
# Default: concise progress
cargo build
   Compiling mycrate v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 0.5s

# Verbose: detailed output
cargo build -v
   Compiling mycrate v0.1.0
     Running `rustc --crate-name mycrate src/lib.rs ...`
    Finished dev [unoptimized + debuginfo] target(s) in 0.5s

# Quiet: errors only
cargo build -q
# (no output on success)
```

### JSON message protocol

cargo can output structured JSON for tooling:

```bash
cargo build --message-format=json
```

Output:
```json
{"reason":"compiler-message","package_id":"mycrate 0.1.0","message":{"$message_type":"diagnostic","level":"error","message":"..."}}
```

This enables:
- IDE integration
- CI parsing
- Custom tooling

### Error formatting

Rust/cargo error messages are industry-leading:

```
error[E0425]: cannot find function `foo` in this scope
 --> src/main.rs:3:5
  |
3 |     foo();
  |     ^^^ not found in this scope
  |
help: a function with a similar name exists
  |
3 |     foo_bar();
  |     ~~~~~~~
```

Features:
- Clear error codes (E0425)
- Precise location (file, line, column)
- Visual indication (underlines)
- Helpful suggestions

### Progress indication

cargo uses progress bars that:
- Show current crate being compiled
- Update in place (no scroll spam)
- Clear on completion
- Handle parallel compilation

```
   Compiling libc v0.2.150
   Compiling serde v1.0.193
   Compiling tokio v1.34.0
```

## 4) Standout strengths

- **Error messages**: Best-in-class formatting and helpfulness
- **Progress bars**: Visible but not noisy
- **Verbosity levels**: Progressive disclosure
- **JSON output**: Machine-readable for tooling
- **Consistency**: Same patterns across all commands
- **Discoverability**: `--help` is comprehensive

## 5) Chronic weaknesses and recurring costs

### Compiler-centric

cargo is tightly coupled to rustc:
- Error format assumes compiler output
- Progress assumes crate compilation
- Less applicable to non-Rust workflows

### Limited concurrent output

cargo buffers compiler output:
- Only shows one crate at a time
- Full output only on error or verbose mode
- Less suitable for long-running concurrent tasks

### No persistent process management

cargo run starts a process but doesn't manage it:
- No restart on change (need cargo-watch)
- No process supervision
- No concurrent process coordination

## 6) Between-release corrections

### Pre-1.0 → 1.0 (2014-2015)
- Basic functionality
- Simple output

### 2016-2020
- Progress bars added
- Error message improvements
- JSON format stabilized

### 2021-2024
- Better diagnostics grouping
- Improved incremental compile feedback
- Artifact caching visibility

The pattern: Continuous refinement of UX based on community feedback.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Progressive output**: Default concise, `-v` for detail, `-q` for silent
- **JSON output mode**: Machine-readable for tooling integration
- **Error formatting**: Clear codes, locations, suggestions
- **In-place progress**: Update without scroll spam
- **Consistency**: Same patterns across commands

### Reject early

- **Compiler-centric assumptions**: Effigy is language-agnostic
- **Limited concurrency**: Effigy manages multiple concurrent processes
- **No persistent processes**: Effigy has watch mode and long-running tasks

### Prototype before deciding

- cargo-style progress bars for Effigy tasks
- JSON output format for `effigy --json`
- Error message formatting inspiration

## 8) Comparison: cargo vs. Effigy

| Aspect | cargo | Effigy |
|--------|-------|--------|
| Language focus | Rust only | Language-agnostic |
| Concurrent output | Buffered (one at a time) | TUI with multiple panes |
| Process management | Run and exit | Supervise, restart, coordinate |
| Error source | Compiler | Task execution |
| Progress indication | Crate-level | Task-level |

**Pattern**: cargo is a compiler driver; Effigy is a process orchestrator. Different goals, but cargo's output patterns are worth studying.

## 9) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [cargo book](https://doc.rust-lang.org/cargo/) | official docs | current | high | Primary reference |
| [cargo source](https://github.com/rust-lang/cargo) | source | current | high | Implementation |
| [Rust diagnostics RFC](https://rust-lang.github.io/rfcs/1644-default-and-speaking-diagnostics.html) | RFC | 2016 | high | Error message design |
| Community feedback | observation | ongoing | high | "cargo has great UX" |

## 10) Open questions

- How does cargo's progress bar implementation handle terminal resizing?
- What's the performance impact of JSON message formatting?
- How does cargo decide what information to show at each verbosity level?

## Next Task

Compare against pnpm and other tools in Track 05 synthesis on TUI patterns.

