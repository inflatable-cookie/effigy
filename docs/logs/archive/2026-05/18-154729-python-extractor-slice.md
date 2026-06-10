# Python Extractor Slice

Date: 2026-05-18  
Roadmap: [`g07.039`](../../../roadmaps/g07/039-richer-language-extractor-coverage.md)  
Batch card: [`988`](../../../roadmaps/g07/batch-cards/988-expand-language-extractor-priority-set.md)  
Strict lane: [`091`](../../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- added first-party Python indexing through `tree-sitter-python`
- registered Python as a built-in graph language with `language_id = "python"`
- emit Python file/module, class, and function symbols
- emit:
  - resolved `import-file` edges for local modules when the target can be
    resolved to `*.py` or `__init__.py`
  - unresolved `import` edges when resolution is not available
  - unresolved `call` edges and `call-site` references for function calls
- keep parser failures diagnostic-only rather than index-fatal
- added regression coverage for both happy-path indexing and parse-failure
  behavior

## Dependency Impact

- one new grammar dependency: `tree-sitter-python v0.25.0`
- no runtime daemon, plugin system, or non-Rust dependency was introduced
- build impact stayed inside the existing tree-sitter posture already used for
  Rust, PHP, and JS/TS extractors

## Validation

- `cargo test -p effigy-codegraph`
- `cargo clippy -p effigy-codegraph -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo fmt --all -- --check`

New regressions:

- `graph_python_indexer_emits_import_call_and_class_facts`
- `graph_python_indexer_emits_parse_diagnostics_without_failing_file`

## Coverage Shape

This slice intentionally stays conservative:

- functions
- classes
- local module imports
- call sites
- parse diagnostics

It does not yet claim:

- type inference
- decorator semantics beyond preserving the wrapped definition
- Flask/Django/FastAPI route facts
- full package-resolution semantics across arbitrary repo layouts

Those route and framework facts belong in `989`.

## Interpretation

- Python is now a first-party graph language, which lifts a major gap in the
  parity lane without changing Effigy's core posture
- the extractor pattern held cleanly for another language family
- traversal and context work now have a broader graph substrate to build on

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: the graph surface now supports Python as a first-party indexed
  language with symbols, imports, calls, references, and diagnostic-only
  parse failures
- remains open: framework route/entrypoint edges, additional language tiers,
  no-reread source packets, affected-test workflow, and final parity proof

## Next Task

Execute `989`.
