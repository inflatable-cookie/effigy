# 1049 - Classify Rust Impl And Associated-Call Dead-Code

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1048`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Reduce remaining function false positives caused by Rust impl methods and
associated/self call forms that the dead-code scan does not yet credit.

## Work

- inventory residual function findings in impl-heavy files such as:
  - `crates/effigy-ui/src/plain_renderer/mod.rs`
  - `crates/effigy-codegraph/src/storage.rs`
  - `crates/effigy-builtin/src/config/docs/tasks.rs`
- add focused fixtures for:
  - trait impl methods with bodies that should not report as standalone
    functions
  - inherent methods reached through `self.method()` or `Type::method()`
  - private free functions that should still report when unused
- implement the smallest scanner/indexer change that credits those impl/call
  relationships
- rerun the self-scan and record before/after counts
- classify remaining function findings as graph gaps or possible real cleanup

## Guardrails

- no blanket function suppression
- no blanket skip for all impl methods
- no file-path or type-name allowlist
- no CI gate
- no JSON schema change
- no broad code deletion from advisory findings

## Acceptance

- one Rust impl/call false-positive class is reduced
- unused private free functions remain eligible findings
- focused tests pass
- final evidence records scan counts and the next residual precision slice

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- focused codegraph tests if Rust extraction changes
- `cargo build -p effigy`
- `target/debug/effigy graph index --json`
- `target/debug/effigy scan dead-code --json`
- scan JSON contract tests if output examples change
- `effigy qa:docs`
- `git diff --check`

## Evidence

- Planning: [`../../../logs/archive/2026-06/04-232018-dead-code-rust-impl-call-planning.md`](../../../logs/archive/2026-06/04-232018-dead-code-rust-impl-call-planning.md)
- Implementation: [`../../../logs/archive/2026-06/04-232355-dead-code-rust-impl-call-precision.md`](../../../logs/archive/2026-06/04-232355-dead-code-rust-impl-call-precision.md)

## Next Task

Planning checkpoint: decide the next residual dead-code batch under `g08.009`.
