# 1044 - Fix Dead-Code Scan Rust Signal

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: operator-directed follow-up after `1043`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Fix the scan weakness found by `1043` instead of relying on repo-level symbol
suppression.

## Work

- resolve unique unresolved edge/reference targets in dead-code analysis
- treat Rust module declarations as evidence that module files are live
- skip Rust public API roots as dead-code symbol candidates
- mark Rust call-site graph facts as syntactic evidence
- replace boundary scan's per-edge symbol search with a lookup
- remove Effigy's dead-code `allow_symbols = ["*"]` workaround

## Guardrails

- no deletion from advisory scan output alone
- no CI gate
- no JSON schema change

## Acceptance

- dead-code output improves without repo-wide symbol suppression
- focused scan and codegraph tests cover the corrected behavior
- residual findings are documented

## Validation

- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- focused codegraph and scan tests

## Evidence

- [`../../../logs/2026-06/04-221542-dead-code-scan-rust-signal-correction.md`](../../../logs/2026-06/04-221542-dead-code-scan-rust-signal-correction.md)

## Next Task

Planning checkpoint: decide whether the remaining 1,178 advisory findings need
another graph-precision tranche or targeted code cleanup.
