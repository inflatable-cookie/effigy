# ESLint

Status: Draft
Tool name: ESLint
Category: linter (rule-based error reporting)
Owner:
Last updated: 2026-03-07
Scope: ESLint rule system, error reporting patterns, configurable diagnostics

## 1) Why this tool matters

ESLint is the standard JavaScript/TypeScript linter. It's notable for:
- Pluggable rule architecture
- Configurable severity (error/warn/off)
- Fixable rules (automatic corrections)
- Extensive ecosystem

For Effigy, ESLint represents:
- Rule-based error systems
- Configurable diagnostics
- Plugin architectures
- Fix suggestions

## 2) Product and era context

### Timeline

- **2013**: ESLint created by Nicholas Zakas
- **2015**: v1.0, ecosystem growth
- **2016-2019**: Standard for JS linting
- **2020-2024**: Flat config rewrite, continued dominance

### Design Philosophy

From ESLint documentation:

> "Pluggable linting utility for JavaScript"
> "Every rule is standalone"
> "Rules are configurable"

### Target Audience

- JavaScript/TypeScript developers
- Teams wanting consistent code style
- Tool builders (ESLint as platform)

### Architecture

ESLint is built on rules:
```javascript
// Example rule
module.exports = {
  meta: {
    type: "problem",
    docs: { description: "disallow unused variables" },
    fixable: "code",
  },
  create(context) {
    return {
      Identifier(node) {
        if (isUnused(node)) {
          context.report({
            node,
            message: "'{{name}}' is defined but never used.",
            data: { name: node.name },
            fix: fixer => fixer.remove(node),
          });
        }
      }
    };
  }
};
```

## 3) Defining architectural bets

### Rule-based architecture

Everything is a rule:
- Built-in rules
- Plugin rules
- Custom rules

Rules have:
- Metadata (type, docs, fixable)
- Visitor functions (AST traversal)
- Reports (errors/warnings)

### Configurable severity

Each rule configurable:
```javascript
// .eslintrc.js
module.exports = {
  rules: {
    "no-unused-vars": "error",      // Fail
    "no-console": "warn",           // Warn
    "quotes": ["error", "single"],  // Error with option
    "semi": "off",                  // Disabled
  }
};
```

Benefits:
- Teams decide what's important
- Gradual adoption
- Project-specific needs

### AST-based analysis

ESLint parses to AST, then traverses:
```javascript
// Code
const x = 1;

// AST (simplified)
{
  type: "VariableDeclaration",
  declarations: [{
    type: "VariableDeclarator",
    id: { type: "Identifier", name: "x" },
    init: { type: "Literal", value: 1 }
  }]
}
```

Rules listen for specific node types.

### Fixable rules

Many rules can auto-fix:
```bash
eslint --fix src/
```

Rules provide `fix()` function:
```javascript
context.report({
  node,
  message: "Missing semicolon",
  fix: fixer => fixer.insertTextAfter(node, ";")
});
```

## 4) Standout strengths

- **Pluggability**: Huge ecosystem of rules
- **Configurability**: Severity, options per rule
- **Auto-fix**: Automatic corrections
- **Editor integration**: LSP support
- **Performance**: Incremental linting
- **Extensibility**: Custom rules easy to write

## 5) Chronic weaknesses and recurring costs

### Configuration complexity

.eslintrc can be complex:
```javascript
module.exports = {
  extends: ["airbnb", "plugin:react/recommended"],
  plugins: ["react", "import"],
  rules: {
    // Hundreds of possible rules
  },
  overrides: [{
    files: ["*.test.js"],
    rules: { "no-undef": "off" }
  }]
};
```

Many teams copy-paste configs without understanding.

### Rule conflicts

Rules can conflict:
- One rule requires semicolons
- Another forbids them
- Requires careful configuration

### Performance at scale

Large codebases:
- Many files to lint
- Many rules to run
- Can be slow without caching

## 6) Between-release corrections

### v5 → v6 (2019)
- Improved configuration resolution
- Better ignore patterns

### v7 → v8 (2021)
- ESLint API changes
- Improved flat config (experimental)

### Flat config (2022-2024)
- New configuration system
- Simpler, more flexible
- Gradual adoption

The pattern: Continuous refinement of configuration system.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Configurable severity**: Error/warn/off for checks
- **Rule-based architecture**: Modular validation
- **Auto-fix**: Suggest corrections
- **Plugin system**: Extensibility

### Reject early

- **AST complexity**: Effigy doesn't need parsing
- **Configuration overload**: Keep simple defaults
- **Rule conflicts**: Design to avoid

### Prototype before deciding

- Validation rules for effigy.toml
- Auto-fix for common issues
- Configurable severity levels

## 8) Comparison: rustc vs. ESLint

| Aspect | rustc | ESLint |
|--------|-------|--------|
| Errors | Compiler failures | Style/bug detection |
| Severity | Error/Warning | Error/Warning/Off |
| Fixable | Limited | Extensive |
| Config | Minimal | Extensive |
| Focus | Correctness | Style + correctness |

**For Effigy**: rustc-style clarity for errors, ESLint-style configurability for validation.

## 9) Effigy Validation Rules (Proposed)

### Configurable severity

```toml
[validation]
unused_tasks = "warn"
circular_deps = "error"
missing_description = "off"
```

### Auto-fix suggestions

```bash
$ effigy doctor --fix

Fixed 2 issues:
- Removed unused task "old-task"
- Added description to task "build"
```

### Rule examples

```rust
// Unused task rule
fn check_unused_tasks(manifest: &Manifest) -> Vec<Diagnostic> {
    manifest.tasks.iter()
        .filter(|t| !is_referenced(t))
        .map(|t| Diagnostic::warning(
            "unused-task",
            format!("Task '{}' is defined but never referenced", t.name),
            Some(t.location),
        ))
        .collect()
}
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [ESLint docs](https://eslint.org/docs/latest/) | official docs | current | high | Primary reference |
| [ESLint rules](https://eslint.org/docs/latest/rules/) | official docs | current | high | Rule reference |
| [Custom rules guide](https://eslint.org/docs/latest/extend/custom-rules) | official docs | current | high | Architecture |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 11) Open questions

- How do teams decide which rules to enable?
- What's the fix acceptance rate?
- How does ESLint handle rule conflicts?

## Next Task

Compare against rustc and other tools in Track 07 synthesis on error patterns.

