# 909 - Implement JavaScript / TypeScript Extractor

Roadmap: [`../009-javascript-typescript-extractor.md`](../009-javascript-typescript-extractor.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Index JavaScript and TypeScript source without adding a JavaScript runtime
dependency.

## Scope

- parse `.js`, `.jsx`, `.ts`, and `.tsx`
- extract imports, exports, functions, classes, methods, interfaces, type
  aliases, enums, and syntactically obvious React components
- resolve deterministic relative imports
- preserve unresolved package and alias imports

## Guardrails

- no TypeScript typechecker in v1
- no bundler-specific resolver in v1
- no Node runtime dependency
- no framework routing magic unless deterministic and explicitly scoped later

## Acceptance

- agents can find JS/TS definitions and import relations
- unresolved module specifiers remain visible
- heuristic edges are marked heuristic

## Next Task

Execute `911`.
