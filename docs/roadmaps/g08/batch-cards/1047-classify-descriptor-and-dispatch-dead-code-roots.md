# 1047 - Classify Descriptor And Dispatch Dead-Code Roots

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1046`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Reduce descriptor/dispatch false positives in the dead-code scan without
weakening normal private function reporting.

## Work

- inventory residual function findings that are reachable through descriptor
  tables or function-pointer fields
- add focused fixtures for:
  - a function assigned to a descriptor-table field that should not report
  - a function assigned to a local/static dispatch table that should not report
  - a private helper function outside the descriptor table that should still
    report
- implement the smallest scanner/indexer change that credits descriptor-owned
  functions
- rerun the self-scan and record before/after counts
- classify remaining DTO/render/config model findings separately

## Guardrails

- no broad function suppression
- no file-path allowlist
- no Effigy-specific descriptor names
- no CI gate
- no JSON schema change
- no broad code deletion from advisory findings

## Acceptance

- one descriptor/dispatch false-positive class is reduced
- private helper functions remain eligible findings
- focused tests pass
- final evidence records scan counts and the next residual precision slice

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- focused codegraph tests if Rust extraction changes
- `cargo build -p effigy`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- `target/debug/effigy scan boundary-violations --json` if resolver behavior
  changes
- `effigy qa:docs`
- `git diff --check`

## Evidence

- Planning: [`../../../logs/2026-06/04-230026-dead-code-descriptor-root-planning.md`](../../../logs/2026-06/04-230026-dead-code-descriptor-root-planning.md)
- Implementation: [`../../../logs/2026-06/04-230651-dead-code-descriptor-root-precision.md`](../../../logs/2026-06/04-230651-dead-code-descriptor-root-precision.md)

## Next Task

Planning checkpoint: decide the next residual dead-code batch under `g08.009`.
