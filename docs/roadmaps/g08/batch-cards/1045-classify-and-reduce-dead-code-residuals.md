# 1045 - Classify And Reduce Dead-Code Residuals

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1044`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Turn the remaining dead-code scan output into a trustworthy queue by reducing
one more graph false-positive class and classifying the rest.

## Work

- inventory the top residual dead-code findings by path, symbol kind, and cause
- choose one bounded graph-precision fix from:
  - test-only symbol handling
  - trait/impl method surface handling
  - path-qualified or associated Rust call matching
- implement the selected fix with focused tests
- rerun the self-scan and record before/after counts
- identify any small, obvious real cleanup candidates separately from graph
  gaps, without deleting broad code from scan output alone

## Guardrails

- no broad code deletion from advisory findings
- no repo-wide symbol suppression
- no CI gate
- no JSON schema change
- no Effigy-only scanner behavior

## Acceptance

- one residual false-positive class is reduced by scanner/indexer behavior
- unused test helpers remain visible as possible cleanup
- remaining findings are classified in evidence
- focused tests pass
- roadmap state records the next queue honestly

## Validation

- focused codegraph and scan tests for the selected fix
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- `target/debug/effigy scan boundary-violations --json` if call confidence or
  resolver behavior changes
- `effigy qa:docs`
- `git diff --check`

## Evidence

- [`../../../logs/2026-06/04-223151-dead-code-test-scope-filter.md`](../../../logs/2026-06/04-223151-dead-code-test-scope-filter.md)

## Next Task

Planning checkpoint: decide the next residual dead-code precision slice.
