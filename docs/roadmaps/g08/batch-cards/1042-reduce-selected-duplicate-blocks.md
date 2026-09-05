# 1042 - Reduce Selected Duplicate Blocks

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1041`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Reduce the highest-confidence duplicate blocks from the sweep without widening
scope into broad cleanup.

## Work

- share scan file-role helpers between dead-code and validation-gap scans
- reduce repeated help-topic option rows where source review stays clear
- converge selected container policy and catalog service fixture builders
- rerun duplicate-block scan and record residual findings

## Guardrails

- no scan semantics changes without tests
- no hidden generated help text
- no public test-support crate unless ownership demands it

## Acceptance

- selected high duplicate findings are removed or explicitly deferred
- fixture helper changes improve clarity
- focused scan/help/fixture tests pass

## Validation

- `effigy scan duplicate-blocks --json`
- focused scan, help, and fixture tests

## Evidence

- [`../../../logs/archive/2026-06/04-214831-selected-duplicate-block-follow-through.md`](../../../logs/archive/2026-06/04-214831-selected-duplicate-block-follow-through.md)

## Next Task

Run [`1043-tune-boundary-and-dead-code-scans-for-effigy.md`](./1043-tune-boundary-and-dead-code-scans-for-effigy.md).
