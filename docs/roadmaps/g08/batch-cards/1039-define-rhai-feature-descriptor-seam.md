# 1039 - Define Rhai Feature Descriptor Seam

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1038`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Bind Rhai feature ids, surface metadata, and dispatch coverage to one
reviewable descriptor layer.

## Work

- add descriptor shape for feature id, module, function name, safety, and
  dispatch coverage
- render surface output from descriptors where safe
- add coverage tests for registered feature ids and runner dispatch
- keep ergonomic helper overloads explicit

## Guardrails

- no Rhai helper removal
- no script grammar changes
- no command behavior change behind helpers

## Acceptance

- every feature id has metadata and dispatch coverage
- surface JSON/text remains stable or intentionally normalized
- focused Rhai tests pass

## Evidence

- [`../../../logs/2026-06/04-210845-rhai-feature-descriptor-seam.md`](../../../logs/2026-06/04-210845-rhai-feature-descriptor-seam.md)

## Validation

- `cargo test -p effigy-rhai`
- focused runner Rhai surface tests

## Next Task

Run `1040`.
