# rustc (Rust compiler)

Status: Draft
Tool name: rustc
Category: compiler (error message gold standard)
Owner:
Last updated: 2026-03-07
Scope: rustc error formatting, diagnostics, suggestions, error recovery

## 1) Why this tool matters

rustc is widely considered to have the best error messages in the industry. The Rust team has invested heavily in:
- Clear error descriptions
- Precise location information
- Helpful suggestions
- Visual formatting with colors and underlines

For Effigy, rustc represents:
- The gold standard for error UX
- Diagnostic formatting patterns
- Suggestion/help systems
- Error code organization

## 2) Product and era context

### Timeline

- **2010**: rustc development begins
- **2012**: Initial error message improvements
- **2015**: Rust 1.0 - "Good error messages" a core value
- **2016**: Major error format overhaul (RFC 1644)
- **2018-2024**: Continuous refinement

### Design Philosophy

From Rust RFC 1644:

> "Good error messages are part of Rust's value proposition"
> "Errors should be: clear, concise, actionable"
> "Help the user understand what went wrong and how to fix it"

### Target Audience

- Rust developers (all levels)
- Tool builders studying error UX
- Compiler designers

## 3) Defining architectural bets

### Structured diagnostics

rustc uses a structured diagnostic system:

```rust
struct Diagnostic {
    level: ErrorLevel,        // Error, Warning, Note, Help
    code: Option<ErrorCode>,  // E0425, etc.
    message: String,
    spans: Vec<Span>,         // Where in code
    suggestions: Vec<Suggestion>,
    children: Vec<Diagnostic>, // Related messages
}
```

This enables consistent formatting across all errors.

### Error codes

Every error has a code:
```
error[E0425]: cannot find value `foo` in this scope
```

Benefits:
- Searchable (Google "rust E0425")
- Link to documentation
- Stable reference

### Visual formatting

Errors include visual indicators:
```
error[E0425]: cannot find value `foo` in this scope
 --> src/main.rs:3:5
  |
3 |     foo();
  |     ^^^ help: a local variable with a similar name exists: `bar`
  |
```

Elements:
- File path and location
- Line numbers
- Caret/underline pointing to error
- Labels on the same line

### Suggestions

rustc often suggests fixes:
```
help: you might be missing a type parameter
  |
5 | fn process<T>(item: T) {
  |           +++
```

Types of help:
- `help:` general advice
- `suggestion:` specific code change
- `note:` additional context

### Error recovery

rustc attempts to continue after errors:
- Parse remaining code
- Report multiple errors
- Don't stop at first problem

This gives users a complete picture of issues.

## 4) Standout strengths

- **Clarity**: Plain language explanations
- **Precision**: Exact location (file, line, column)
- **Visual**: Colors and underlines
- **Actionable**: Specific suggestions
- **Comprehensive**: Multiple related errors
- **Consistent**: Same format everywhere

## 5) Chronic weaknesses and recurring costs

### Complexity

rustc's diagnostic system is sophisticated:
- ~50K lines of diagnostic code
- Multiple output formats (short, long, JSON)
- Localization challenges

### "Wall of text"

Complex errors can be overwhelming:
```
error[E0107]: this struct takes 3 generic arguments but 2 were supplied
  --> src/main.rs:5:12
   |
5  |     let x: MyStruct<i32, String>;
   |            ^^^^^^^ expected 3 generic arguments
   |
help: add missing generic argument
   |
5  |     let x: MyStruct<i32, String, T>;
   |                     +++++++++++
note: struct defined here
  --> src/lib.rs:1:1
   |
1  | struct MyStruct<T, U, V>;
   |        ^^^^^^^
```

Can be verbose for simple mistakes.

### Maintenance

Error messages need ongoing work:
- New errors need good messages
- Existing messages need refinement
- Edge cases discovered by users

## 6) Between-release corrections

### Pre-RFC 1644 (2015)
- Basic error messages
- Limited formatting

### Post-RFC 1644 (2016+)
- Structured diagnostics
- Visual formatting
- Suggestions

### Modern rustc (2020-2024)
- `#[must_use]` improvements
- Better async error messages
- Const generics diagnostics

The pattern: Continuous investment in error UX.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Clear language**: Explain what went wrong
- **Precise location**: File, line, context
- **Visual formatting**: Colors, underlines
- **Suggestions**: Help users fix issues
- **Error codes**: Searchable references
- **Consistency**: Same format throughout

### Reject early

- **Compiler-centric complexity**: Effigy is simpler
- **Localization scope**: Start with English
- **Recovery parsing**: Not applicable

### Prototype before deciding

- Error format for task failures
- Suggestion system for common mistakes
- JSON error output for tooling

## 8) Comparison: Error Styles

| Tool | Strength | Weakness |
|------|----------|----------|
| rustc | Visual, detailed, helpful | Can be verbose |
| gcc/clang | Technical, concise | Less helpful |
| Go | Simple | Minimal guidance |

**For Effigy**: Aim for rustc-style clarity without compiler complexity.

## 9) Effigy Error Format (Proposed)

### Task failure format

```
error: Task "build" failed
  --> effigy.toml:12
   |
12 | run = "cargo build --release"
   |       ^^^^^^^^^^^ command not found
   |
   = note: cargo is required for this task
   = help: Install Rust: https://rustup.rs/
```

### Multiple errors

```
error: Task "test" failed with 2 errors
  --> effigy.toml:15
   |
15 | run = "npm test"
   |       ^^^ command not found
   |
   = note: npm is required for Node.js tasks

error: Task "lint" depends on failed task "build"
  --> effigy.toml:20
   |
20 | run = [{ task = "build" }, "eslint ."]
   |              ^^^^^ dependency failed
```

### JSON output

```bash
effigy build --json
```

```json
{
  "status": "failure",
  "errors": [{
    "code": "E001",
    "message": "Task failed",
    "location": {"file": "effigy.toml", "line": 12},
    "context": "run = \"cargo build\"",
    "help": "cargo is not installed"
  }]
}
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [rustc diagnostics](https://rustc-dev-guide.rust-lang.org/diagnostics.html) | official docs | current | high | Implementation |
| [RFC 1644](https://rust-lang.github.io/rfcs/1644-default-and-speaking-diagnostics.html) | RFC | 2016 | high | Design rationale |
| [Rust error codes](https://doc.rust-lang.org/error_codes/) | official docs | current | high | Error reference |
| rustc source | source | current | high | Implementation |

## 11) Open questions

- How does rustc decide what context to show?
- What's the performance cost of rich diagnostics?
- How are error messages tested?

## Next Task

Compare against ESLint and other tools in Track 07 synthesis on error patterns.

