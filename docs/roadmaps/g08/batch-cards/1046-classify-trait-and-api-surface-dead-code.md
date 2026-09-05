# 1046 - Classify Trait And API Surface Dead-Code

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1045`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Reduce the remaining method/trait dead-code noise without weakening the scan's
ability to find genuinely unused private methods.

## Work

- inventory residual method and trait findings by declaration role
- add focused fixtures for:
  - trait method declarations that should not report as standalone dead code
  - required trait impl methods that should not report when the trait surface is
    reachable
  - unused private inherent methods that should still report
  - unused test helpers that should still report
- implement the smallest scanner/indexer change that fixes the selected trait
  or API-surface false-positive class
- rerun the self-scan and record before/after counts
- classify the remaining residual queue after the fix

## Guardrails

- no broad method suppression
- no broad trait suppression
- no Effigy-specific allowlist
- no CI gate
- no JSON schema change
- no broad code deletion from advisory findings

## Acceptance

- one trait/API surface false-positive class is reduced
- private inherent methods remain eligible findings
- unused test helpers remain eligible findings
- focused tests pass
- final evidence records scan counts and the next residual precision slice

## Validation

- `cargo test -p effigy scan_tests::dead_code -- --nocapture`
- focused codegraph tests if Rust indexing changes
- `cargo build -p effigy`
- `target/debug/effigy scan dead-code --json`
- `target/debug/effigy scan boundary-violations --json` if resolver behavior
  changes
- `effigy qa:docs`
- `git diff --check`

## Evidence

- Planning: [`../../../logs/archive/2026-06/04-225013-dead-code-trait-surface-planning.md`](../../../logs/archive/2026-06/04-225013-dead-code-trait-surface-planning.md)
- Implementation: [`../../../logs/archive/2026-06/04-225805-dead-code-trait-surface-precision.md`](../../../logs/archive/2026-06/04-225805-dead-code-trait-surface-precision.md)

## Next Task

Run [`1047-classify-descriptor-and-dispatch-dead-code-roots.md`](./1047-classify-descriptor-and-dispatch-dead-code-roots.md).
