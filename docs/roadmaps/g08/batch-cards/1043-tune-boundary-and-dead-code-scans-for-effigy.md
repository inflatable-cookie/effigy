# 1043 - Tune Boundary And Dead-Code Scans For Effigy

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1042`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Make graph-aware scan results actionable for Effigy's own maintenance queue.

## Work

- add or explicitly defer Effigy boundary layer config
- classify dead-code scan noise into indexing gaps, public API false positives,
  and real investigation candidates
- add suppressions or labels only with evidence
- record which findings can become gates later

## Guardrails

- no deletion from advisory scan output alone
- no CI gate
- no Effigy-only scanner behavior

## Acceptance

- boundary scan is useful for at least one Effigy layer set, or deferral is
  documented
- dead-code output is less noisy or better classified
- closeout leaves a clear next maintenance queue

## Validation

- `effigy scan boundary-violations --json`
- `effigy scan dead-code --json`
- focused graph-aware scan tests

## Evidence

- [`../../../logs/2026-06/04-215614-boundary-dead-code-self-adoption.md`](../../../logs/2026-06/04-215614-boundary-dead-code-self-adoption.md)

## Next Task

Planning checkpoint: decide the next `g08` tranche from the completed sweep
evidence.
