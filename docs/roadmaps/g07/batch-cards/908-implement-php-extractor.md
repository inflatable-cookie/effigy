# 908 - Implement PHP Extractor

Roadmap: [`../008-php-extractor.md`](../008-php-extractor.md)
Strict lane: [`../../../specs/085-code-graph-intelligence-strict-lane.md`](../../../specs/085-code-graph-intelligence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-17

## Purpose

Index PHP source well enough for legacy and Decodelabs-style application work
without product-specific core behavior.

## Scope

- parse `.php` and `.phtml`
- extract namespaces, classes, interfaces, traits, methods, functions,
  constants, imports, includes, and requires
- emit containment, namespace/import, and static include edges
- emit heuristic call-like references where syntactically obvious

## Guardrails

- no PHP runtime execution
- no Composer autoloader evaluation
- no framework boot
- no Decodelabs-specific product names in core extractor behavior
- dynamic behavior stays heuristic or unresolved

## Acceptance

- agents can navigate PHP class/function/file ownership
- include-heavy legacy code produces useful graph facts
- front-controller patterns are represented generically

## Next Task

Execute `909`.
