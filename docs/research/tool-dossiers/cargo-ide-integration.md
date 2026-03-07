# cargo (IDE Integration)

Status: Draft
Tool name: cargo
Category: Rust build tool (IDE integration focus)
Owner:
Last updated: 2026-03-07
Scope: cargo IDE integration, rustc error format, JSON output

## 1) Why this tool matters

cargo has excellent IDE integration. It's notable for:
- JSON output for machine consumption
- Standardized error format
- Language Server Protocol integration (rust-analyzer)
- Rich metadata for IDE features

For Effigy, cargo represents:
- Machine-parseable output patterns
- IDE-friendly error formats
- Tooling ecosystem integration
- JSON vs. human-readable tradeoffs

## 2) Product and era context

### Timeline

- **2014**: cargo initial release
- **2016**: JSON output added (`--message-format=json`)
- **2018**: rust-analyzer project started
- **2020**: rust-analyzer becomes official
- **2022**: Diagnostics JSON format stabilized

### Design Philosophy

From cargo documentation:

> "The JSON output is intended for tools to consume"
> "Human-readable output is the default"

### Target Audience

- Rust developers
- IDE/Editor authors
- Build tool integrators

### Ecosystem

- **rust-analyzer**: Official LSP implementation
- **IntelliJ Rust**: JetBrains plugin
- **coc-rust-analyzer**: Vim plugin
- **VS Code**: Official extension

## 3) Defining architectural bets

### JSON message format

Machine-parseable build output:

```bash
cargo build --message-format=json
```

```json
{
  "reason": "compiler-message",
  "package_id": "myapp 0.1.0",
  "target": {...},
  "message": {
    "message": "unused variable",
    "level": "warning",
    "spans": [{
      "file_name": "src/main.rs",
      "byte_start": 10,
      "byte_end": 14,
      "line_start": 5,
      "line_end": 5,
      "column_start": 9,
      "column_end": 13
    }]
  }
}
```

Benefits:
- Structured data
- No parsing ambiguity
- Extensible

### Error format consistency

rustc errors follow consistent pattern:
```
error[E0382]: borrow of moved value
  --> src/main.rs:15:14
   |
14 |     let s = String::from("hello");
   |         - move occurs because `s` has type `String`
15 |     println!("{}", s);
   |              ^ value borrowed here after move
   |
   = help: consider cloning the value
```

Components:
- Error code (E0382)
- Message
- Location
- Context (code snippet)
- Help text

### LSP integration

rust-analyzer provides:
- Code completion
- Go to definition
- Find references
- Diagnostics from cargo check
- Run/test code lenses

### Metadata commands

cargo provides metadata for tools:
```bash
cargo metadata --format-version 1  # Project structure
cargo tree                          # Dependency tree
cargo pkgid                         # Package ID
```

## 4) Standout strengths

- **JSON output**: Machine-parseable
- **Error codes**: Systematic error identification
- **Span information**: Precise source locations
- **LSP integration**: First-class IDE support
- **Metadata commands**: Tool-friendly introspection
- **Documentation**: rustdoc integration

## 5) Chronic weaknesses and recurring costs

### JSON verbosity

JSON output is verbose:
```json
// Many fields, deeply nested
// Harder to read than human output
// Larger output size
```

### Build vs. check separation

cargo check for IDE, cargo build for actual builds:
```bash
cargo check  # Fast, for IDE
cargo build  # Slower, produces artifacts
```

Need to run both or choose.

### Compiler lock-in

Tight coupling with rustc:
- Error format is rustc-specific
- Hard to adapt for other languages
- Build system assumptions

## 6) Between-release corrections

### Early cargo (2014-2016)
- Human-readable output only
- Limited IDE support

### Modern cargo (2017-2024)
- JSON message format
- rust-analyzer integration
- Better LSP support
- Metadata commands

The pattern: From human-only to machine-friendly.

## 7) Effigy-relevant lessons

### Adopt carefully

- **JSON output**: Machine-parseable for IDE integration
- **Error codes**: Systematic identification
- **Span information**: Precise locations
- **Structured diagnostics**: Multiple severity levels

### Reject early

- **Compiler-specific format**: Keep language-agnostic
- **Tight coupling**: Design for multiple languages
- **Verbosity**: Balance detail with readability

### Prototype before deciding

- Effigy JSON output format
- Error code system
- IDE extension patterns

## 8: Effigy IDE Integration

### Option 1: JSON output

```bash
effigy build --format json
```

```json
{
  "task": "build",
  "status": "success|failure",
  "duration_ms": 1234,
  "outputs": [
    {
      "type": "diagnostic",
      "level": "error|warning|info",
      "code": "E001",
      "message": "...",
      "location": {
        "file": "src/main.rs",
        "line": 10,
        "column": 5
      }
    }
  ]
}
```

### Option 2: Error codes

```
error[E001]: Task failed
  --> effigy.toml:15
   |
15 | command = "unknown-cmd"
   |           ^^^^^^^^^^^^^ command not found
   |
   = help: Run `effigy doctor` to check your installation
```

### Option 3: LSP-like protocol

For advanced IDE features:
- Task discovery
- Configuration validation
- Autocompletion for task names

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [cargo docs](https://doc.rust-lang.org/cargo/) | official docs | current | high | Primary reference |
| [rustc diagnostics](https://rustc-dev-guide.rust-lang.org/diagnostics.html) | docs | current | high | Error format |
| [rust-analyzer](https://rust-analyzer.github.io/) | project docs | current | high | LSP integration |
| cargo source | source | latest | high | Implementation |

## 10: Open questions

- What JSON format balance is right for Effigy?
- Should Effigy provide a language server?
- How to handle multi-language projects?

## Next Task

Compare against LSP and other IDE tools in Track 13 synthesis.

