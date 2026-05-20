# g07.065 - Manifest Semantic Owner Split

Status: Planned
Depends on: `g07.064`

## Goal

Reduce `crates/effigy-codegraph/src/language/manifest/semantic.rs` until it no
longer acts as one large mixed owner.

## Evidence

The current god-file scan still reports:

- `crates/effigy-codegraph/src/language/manifest/semantic.rs`

This file now holds multiple concerns at once:

- section-level manifest interpretation
- relation emission across bundles/tasks/services/runtime-like surfaces
- helper rules for path, command, and template-backed semantics

## Scope

- split semantic manifest indexing by fact family or manifest domain
- preserve graph facts, IDs, and JSON-facing behavior
- keep parsing/template support where it already lives unless a clearer local
  boundary appears
- add focused graph tests for any moved semantic owner

## Guardrails

- no graph storage schema changes
- no semantic ranking rewrite
- no public CLI JSON changes
- no migration of unrelated extractor logic into manifest modules

## Suggested Implementation Shape

- keep `language/manifest/mod.rs` thin
- introduce local semantic owners such as:
  - `tasks.rs`
  - `services.rs`
  - `runtime.rs`
  - `bundles.rs`
  - `relations.rs`
- use names that match the emitted fact family rather than generic `part1`
  splits

## Acceptance Criteria

- `semantic.rs` is no longer a god-file finding, or any remaining large file is
  explicitly justified with a tighter ownership story
- graph extractor tests stay green
- the resulting file tree explains manifest graph ownership at a glance

## Next Task

After this lands, proceed to [`066-codegraph-test-harness-decomposition.md`](./066-codegraph-test-harness-decomposition.md).
