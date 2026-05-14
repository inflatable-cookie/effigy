# g06.005 - CLI Help And Rendering Deduplication

Status: Complete
Depends on: `g06.001`

## Goal

Reduce repeated CLI help and output-rendering scaffolding without centralizing
all content into one brittle registry.

## Evidence

- the duplicate-block sweep still shows high findings in CLI help topic files
- several command surfaces repeat JSON/text layout machinery around otherwise
  local content
- this is good trim work when the repeated logic is layout, not behavior

## Scope

- extract small table-driven helpers for repeated help topic section layout
- extract repeated rendering scaffolds where multiple command families share
  the same framing behavior
- keep command/help content local to its owning module
- retain stable output ordering and spacing

## Out Of Scope

- no global mega registry for all help text
- no rewrite of every command renderer
- no user-facing copy refresh unless needed by the dedupe
- no contract changes to JSON output

## Guardrails For A Cheaper Model

- centralize layout machinery only, not the entire content model
- keep diff readability high
- preserve exact spacing and heading order unless tests intentionally update it
- prefer compile-time simple data over macro-heavy abstractions

## Suggested Implementation Steps

1. Use duplicate scan output to pick the highest-value help/render blocks.
2. Extract the smallest helper that can remove the duplication.
3. Migrate one command family at a time.
4. Re-run focused help/render tests after each batch.

## Acceptance Criteria

- duplicate help/render scaffolding is reduced
- help topic content remains local and readable
- text and JSON output contracts remain stable
- retained repeated sections are explicitly justified

## Validation

Minimum focused validation:

```bash
cargo test help
cargo test tasks_rendering
effigy scan duplicate-blocks --json
```

## Current State

The highest-value help-topic layout duplication slice is landed:

- the heaviest topic render boilerplate now uses one shared spec-driven path
- topic copy remains local in owner files
- duplicate-block high findings dropped again

Smaller remaining help overlaps still exist, but the next stronger lean-down
target is reused JSON contract-shape assembly rather than more help-only
cleanup.

## Next Task

Continue with `g06.006`.
