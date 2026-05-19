# g07.057 - Codegraph Language Emitter Deduplication

Status: Planned
Depends on: `g07.056`

## Goal

Remove duplicated graph-record construction across language extractors while
leaving each language's parsing and semantic decisions local.

## Evidence

The audit found critical duplicate blocks across:

- `crates/effigy-codegraph/src/language/javascript.rs`
- `crates/effigy-codegraph/src/language/php.rs`
- `crates/effigy-codegraph/src/language/python.rs`

The repeated code builds parse diagnostics, symbol records, contains edges,
unresolved edges, and reference records.

## Scope

- introduce a small shared emitter/helper module under `effigy-codegraph`
- move only graph-record construction boilerplate into shared helpers
- keep node traversal, language-specific IDs, symbol-kind decisions, and
  heuristic rules inside each language extractor
- preserve all existing public graph JSON output
- preserve language-specific provenance and range data
- add focused regression tests for one representative extractor per helper

## Guardrails

- do not create a generic language extraction framework
- do not flatten language-specific extractor files into one shared path
- do not change symbol IDs, relation names, source ranges, or diagnostic
  contract fields unless a test proves the current value is wrong
- do not optimize query speed in this card
- do not expand language coverage in this card

## Suggested Implementation Shape

- add `crates/effigy-codegraph/src/language/emit.rs`
- expose helpers for:
  - parse diagnostics
  - symbol records
  - contains edges
  - unresolved edges/references
  - file/language-scoped ID prefix construction where duplicated
- migrate JavaScript and PHP first because they carry the largest exact
  duplicate block
- migrate Python only where the helper matches naturally
- rerun duplicate scan and targeted graph tests

## Acceptance Criteria

- duplicate scan no longer reports critical blocks for JS/PHP/Python emitter
  boilerplate
- existing extractor tests pass
- generated graph facts keep stable IDs and payload fields
- helper names make the emitted graph fact obvious without hiding language
  semantics

## Next Task

After this lands, proceed to [`058-codegraph-manifest-query-module-decomposition.md`](./058-codegraph-manifest-query-module-decomposition.md).
