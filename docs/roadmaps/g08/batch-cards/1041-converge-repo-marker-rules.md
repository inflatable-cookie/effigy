# 1041 - Converge Repo Marker Rules

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1040`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Remove production drift around Effigy's stable repo marker names and pure
root-marker predicates.

## Work

- inventory production marker literals
- choose the lowest-dependency owner for marker constants and predicates
- migrate safe call sites
- keep manifest parsing and loading boundaries unchanged

## Guardrails

- no root-resolution behavior change
- no manifest grammar change
- no new crate for constants alone

## Acceptance

- production marker names have one canonical owner
- duplicated pure predicates are reduced or explicitly deferred
- focused root-resolution and runtime-discovery tests pass

## Validation

- `cargo test -p effigy-core`
- `cargo test -p effigy-routing`
- focused runtime discovery tests

## Evidence

- [`../../../logs/archive/2026-06/04-214009-repo-marker-root-rule-convergence.md`](../../../logs/archive/2026-06/04-214009-repo-marker-root-rule-convergence.md)

## Next Task

Run [`1042-reduce-selected-duplicate-blocks.md`](./1042-reduce-selected-duplicate-blocks.md).
