# g07.004 - First-Party Language Extractor Framework

Status: Complete
Depends on: `g07.003`

## Goal

Create the internal extractor framework used by first-party language indexers.

This is not a plugin system. It is a clean internal boundary so extractors do
not leak storage, query, or CLI concerns into language-specific code.

## Scope

- add an internal `LanguageIndexer` trait
- add a graph sink API for emitted records
- add shared source range and location types
- add extractor diagnostics
- add per-file extraction error isolation
- add extractor versioning
- add tree-sitter integration helpers where useful
- add fixture harnesses for language extractor tests

## Suggested Trait Shape

```rust
trait LanguageIndexer {
    fn id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn index_file(&self, file: &SourceFile, sink: &mut GraphSink) -> Result<()>;
}
```

Keep storage out of the trait. Extractors emit normalized records. Core stores
them.

## Tree-Sitter Guidance

Tree-sitter gives syntax, not semantic truth.

Use it for:

- definitions
- imports/includes
- syntactic references
- doc comments
- route-like static declarations where deterministic

Do not infer type-checked call graphs unless the language makes the relation
obvious from syntax.

## Non-Goals

- no external process extractor protocol
- no dynamic library loading
- no language package resolution
- no compiler/LSP integration
- no language-specific extractor completion in this lane

## Tests

- a fake extractor that emits symbols/edges
- diagnostic propagation
- malformed file isolation
- source range validation
- extractor version freshness interaction

## Acceptance Criteria

- adding a new first-party extractor does not require touching CLI or DB internals
- extractor output is validated before storage
- extractor failures are diagnostics, not whole-index crashes
- edge confidence is explicit

## Next Task

Continue `g07.006`.
