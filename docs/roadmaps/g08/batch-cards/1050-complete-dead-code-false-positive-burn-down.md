# 1050 - Complete Dead-Code False-Positive Burn-Down

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: completed after `1049`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Finish the current dead-code residual burn-down by handling the last false
positive classes and deleting confirmed dead artifacts.

## Work

- credit function references hidden in macro bodies and function-value
  arguments
- credit binary `main` entrypoints
- credit serde default helper references
- credit private traits referenced by impl headers
- credit module files under nested `*/src/` crate paths
- credit cross-file Rust references for symbols that otherwise become
  candidates
- delete remaining confirmed dead artifacts
- rerun the self-scan and record the final candidate count

## Guardrails

- no blanket symbol suppression
- no repo-specific allowlist
- no CI gate change
- no JSON schema change
- delete only symbols/files proven unused after scanner precision fixes

## Acceptance

- current dead-code scan has no false positives
- current dead-code scan has no remaining findings
- focused tests pass
- deleted artifacts are validated by build/tests

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- `cargo build -p effigy`
- `cargo clippy -p effigy-builtin --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- scan JSON contract tests
- `effigy qa:docs`
- `git diff --check`

## Evidence

- Implementation: [`../../../logs/2026-06/04-233355-dead-code-final-burn-down.md`](../../../logs/2026-06/04-233355-dead-code-final-burn-down.md)

## Next Task

No current dead-code residual batch remains.
