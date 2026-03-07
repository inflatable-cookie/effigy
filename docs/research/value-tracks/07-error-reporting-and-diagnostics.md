# Track 07: Error Reporting and Diagnostics

Status: Draft
Track: Error Reporting and Diagnostics
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `UX`, `CLI`

## 1) Problem statement

How should errors be reported? What balances:
- Clarity (users understand what went wrong)
- Actionability (users know how to fix it)
- Conciseness (not overwhelming)
- Programmatic access (for tooling)

## 2) Why this track matters to Effigy

Effigy needs to report:
- Task failures
- Configuration errors
- Dependency issues
- Validation problems

Research validates:
- Error formatting patterns
- Suggestion systems
- Severity levels
- JSON output for automation

## 3) Cross-tool comparison

| Tool | Error Style | Strengths | Weaknesses |
|------|-------------|-----------|------------|
| rustc | Visual, detailed | Clear, helpful, precise | Can be verbose |
| ESLint | Rule-based | Configurable, fixable | Complex config |
| Go | Simple, minimal | Easy to read | Not always helpful |
| gcc | Technical | Concise | Less guidance |

### Error Spectrum

**Minimal (Go)**
```
./main.go:3:5: undefined: foo
```
- Pros: Short, scannable
- Cons: No context, no help

**Detailed (rustc)**
```
error[E0425]: cannot find value `foo` in this scope
 --> src/main.rs:3:5
  |
3 |     foo();
  |     ^^^ help: a local variable with a similar name exists: `bar`
```
- Pros: Clear, actionable
- Cons: Verbose

**Configurable (ESLint)**
```
3:5  warning  'foo' is assigned but never used  no-unused-vars
```
- Pros: Configurable severity
- Cons: Less context

## 4) Repeated patterns

### Universal error requirements

1. **Clear message**
   - What went wrong
   - Plain language

2. **Location**
   - File, line, column
   - Context (surrounding lines)

3. **Severity**
   - Error (blocks execution)
   - Warning (allows continuation)
   - Info/Note (additional context)

4. **Help/Suggestions**
   - How to fix
   - Related documentation

### Tool-specific innovations

**rustc: Structured diagnostics**
```rust
struct Diagnostic {
    level: ErrorLevel,
    code: ErrorCode,
    message: String,
    spans: Vec<Span>,
    suggestions: Vec<Suggestion>,
}
```

**ESLint: Configurable rules**
```javascript
{
  "rules": {
    "no-unused-vars": "warn",
    "no-console": "error"
  }
}
```

**Both: JSON output for tooling**
```bash
$ tool --json
{ "errors": [...] }
```

## 5) Frontier research signals

- **AI-powered explanations**: Error explanation assistants
- **Interactive fixes**: Apply fixes in IDE
- **Error documentation**: Auto-generated help
- **Visual error flows**: Diagrams of what went wrong

## 6) Effigy implications

### Recommended direction

**rustc-inspired format with ESLint-style configurability:**

1. **Clear error format**
   ```
   error: Task "build" failed
     --> effigy.toml:12
      |
   12 | run = "cargo build"
      |       ^^^ command not found
      |
      = note: cargo is required
      = help: Install from https://rustup.rs
   ```

2. **Configurable severity** (for doctor/validation)
   ```toml
   [validation]
   unused_tasks = "warn"
   circular_deps = "error"
   ```

3. **JSON output**
   ```bash
   effigy build --json
   ```

4. **Error codes**
   ```
   error[E001]: Task failed
   ```

### Risks to avoid

1. **Too verbose**: Don't overwhelm users
2. **Too terse**: Don't be unhelpful
3. **Inconsistent formats**: Same style everywhere
4. **No JSON mode**: Breaks tooling

### Evidence or prototype needed

- [ ] Error format user testing
- [ ] JSON schema definition
- [ ] Error code system

## 7) Implementation suggestions

### Error structure

```rust
struct Error {
    code: ErrorCode,        // E001, E002, etc.
    severity: Severity,     // Error, Warning
    message: String,        // Short description
    location: Location,     // file, line, column
    context: String,        // Surrounding lines
    help: Option<String>,   // How to fix
    notes: Vec<String>,     // Additional context
}
```

### Display format

```rust
impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result {
        writeln!(f, "error[{}]: {}", self.code, self.message)?;
        writeln!(f, "  --> {}:{}:{}", 
                 self.location.file,
                 self.location.line,
                 self.location.column)?;
        writeln!(f, "   |")?;
        writeln!(f, "{:2} | {}", 
                 self.location.line,
                 self.context)?;
        writeln!(f, "   | {:>width$} {}", 
                 "^".repeat(self.span_len),
                 self.help.as_deref().unwrap_or(""),
                 width = self.location.column)?;
        Ok(())
    }
}
```

### JSON output

```rust
#[derive(Serialize)]
struct JsonError {
    code: String,
    message: String,
    location: Location,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
}
```

## 8) Comparison: Approaches

| Approach | Pros | Cons | Effigy |
|----------|------|------|--------|
| Minimal (Go) | Simple | Unhelpful | ❌ |
| Detailed (rustc) | Clear, actionable | Verbose | ✅ Inspired |
| Configurable (ESLint) | Flexible | Complex | ✅ For validation |

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| rustc dossier | high | Error formatting |
| ESLint dossier | high | Configurable rules |
| rustc RFC 1644 | high | Design rationale |

## 10) Decision state

- [ ] `promote to concept work` — Document error format
- [ ] `continue research` — Sufficient guidance
- [ ] `prototype first` — Test error format

**Current leaning**: Prototype first — implement rustc-style format, test with users.

## Next Task

1. Draft Translation Memo 007: Error Reporting Strategy
2. Begin Track 08: Monorepo Workspaces

