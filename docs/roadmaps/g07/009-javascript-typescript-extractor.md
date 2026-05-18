# g07.009 - JavaScript / TypeScript Extractor

Status: Complete
Depends on: `g07.004`

## Goal

Index JavaScript and TypeScript source well enough for frontend and full-stack
repo navigation.

## Scope

- parse `.js`, `.jsx`, `.ts`, and `.tsx`
- extract:
  - imports
  - exports
  - functions
  - classes
  - methods
  - interfaces
  - type aliases
  - enums
  - React components where syntactically obvious
- emit import/export edges
- emit file -> symbol containment
- emit heuristic call-like references where obvious
- record unresolved module specifiers instead of guessing

## Resolution Guidance

JS/TS resolution is broad. Start conservative.

Support deterministic cases first:

- relative imports
- explicit file extensions
- common index resolution where file exists

Retain unresolved module specifiers for package imports and aliased paths until
the repo config/indexer can prove a mapping.

## Non-Goals

- no TypeScript typechecker integration in v1
- no bundler-specific module resolver in v1
- no JSX semantic interpretation beyond syntactic component extraction
- no framework routing magic unless deterministic and explicitly scoped later

## Tests

- ESM import/export fixtures
- CommonJS fixture if supported
- React component fixture
- type/interface fixture
- unresolved package import fixture
- relative import resolution fixture

## Acceptance Criteria

- agents can find JS/TS definitions and imports quickly
- unresolved module specifiers are visible, not hidden
- call-like edges are marked heuristic
- extractor handles mixed JS/TS repos without requiring Node

## Next Task

Execute `g07.011`.
