# g07.039 - Richer Language Extractor Coverage

Status: Complete
Depends on: `g07.038`

## Goal

Expand first-party language coverage in the order most likely to improve real
agent navigation.

## Priority Order

1. Python: functions, classes, imports, decorators, framework hooks
2. Go: packages, functions, methods, imports, interface-ish references
3. Java: packages, classes, methods, annotations, imports
4. C# or Ruby: choose based on current fixture/user repo demand
5. C/C++ headers and source: functions, includes, class/method skeletons
6. Swift/Kotlin/Dart only after earlier tiers prove the extractor pattern

## Scope

- evaluate Rust-native parser options before adding each language
- prefer tree-sitter Rust crates or mature Rust parsers where available
- keep extractor failures diagnostic-only, not index-fatal
- preserve provenance, source ranges, confidence, and extractor version
- add mixed-language fixtures for cross-language task ownership where practical
- document unsupported language behavior clearly

## Guardrails

- no JavaScript runtime dependency
- no external language plugin package system
- no "full compiler" claim
- no language added without tests and failure diagnostics
- no broad dependency addition without license/build review

## Acceptance Criteria

- at least Python and Go are fully scoped before implementation begins
- each implemented language has fixture coverage and extractor diagnostics
- benchmark tasks include at least one cross-language query that improves
- package/build impact is measured before closeout

## Evidence

- [`2026-05/18-154729-python-extractor-slice.md`](../logs/2026-05/18-154729-python-extractor-slice.md)

## Next Task

Execute `989` to add framework route and entrypoint edges on top of the wider
language base.
