# 905 - Implement Rust Extractor

Roadmap: [`../005-rust-extractor.md`](../005-rust-extractor.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Index Rust source well enough for agents to navigate Effigy itself.

## Scope

- parse `.rs` files with native tree-sitter Rust support
- extract modules, functions, structs, enums, traits, impls, methods, and uses
- emit containment and syntactic import edges
- emit clearly heuristic call-like references where safe
- preserve short doc comments/snippets within limits

## Guardrails

- no rustc integration
- no proc-macro expansion
- no type or trait resolution claims
- unresolved references stay unresolved

## Acceptance

- Effigy repo indexing produces useful Rust symbol coverage
- Rust edges carry confidence and source ranges
- fixture tests cover modules, impls, traits, imports, and parse errors

## Next Task

Execute `906`.
