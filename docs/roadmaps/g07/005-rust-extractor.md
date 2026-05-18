# g07.005 - Rust Extractor

Status: Complete
Depends on: `g07.004`

## Goal

Index Rust source well enough for agents to navigate Effigy itself.

The target is high-signal syntactic navigation, not compiler-grade semantic
analysis.

## Scope

- parse `.rs` files with native tree-sitter Rust support
- extract:
  - modules
  - functions
  - structs
  - enums
  - traits
  - impl blocks
  - methods
  - `use` items
  - macro definitions where easy
- emit containment edges such as file -> symbol and impl -> method
- emit import/use edges with unresolved targets when needed
- emit heuristic call-like references where syntactically obvious
- preserve doc comments and short snippets within configured limits

## Confidence Rules

- definitions are `syntactic`
- file containment is `exact`
- `use` edges are `syntactic`
- call-like references without type resolution are `heuristic`

## Non-Goals

- no rustc integration
- no proc-macro expansion
- no trait resolution
- no borrow/type analysis
- no complete call graph claim

## Tests

- fixture crate with modules, traits, impls, and nested modules
- imports across files
- same-name functions in different modules
- macro-heavy file degrades gracefully
- extractor diagnostics for parse errors

## Acceptance Criteria

- indexing Effigy itself produces useful Rust symbol and module coverage
- an agent can find key Rust owners without scanning every file
- unresolved edges are preserved as unresolved, not guessed as facts
- all emitted edges carry confidence and source ranges

## Next Task

Continue `g07.006`.
