# 1048 - Classify DTO Render Config Dead-Code Roots

Roadmap: [`../009-code-quality-boundary-sweep-suite.md`](../009-code-quality-boundary-sweep-suite.md)
Strict lane: ready after `1047`

Status: Complete
Owner: Platform
Created: 2026-06-04
Completed: 2026-06-04

## Purpose

Reduce DTO/render/config data-shape false positives in the dead-code scan
without hiding ordinary private structs, enums, or helper functions.

## Work

- inventory residual struct and enum findings that are used as serialization,
  rendering, config, or generated-policy data shapes
- add focused fixtures for:
  - private serializable payload structs that should not report when they are
    constructed or returned through serde/render surfaces
  - private config/render row structs that should not report when used as data
    model roots
  - unused private structs/enums that should still report
- implement the smallest scanner/indexer change that credits data-shape roots
- rerun the self-scan and record before/after counts
- classify remaining findings after data-shape roots are reduced

## Guardrails

- no blanket struct or enum suppression
- no file-path allowlist for release, tasks, runtime, or generated-compose code
- no repo-specific type-name allowlist
- no CI gate
- no JSON schema change
- no broad code deletion from advisory findings

## Acceptance

- one DTO/render/config false-positive class is reduced
- unused private structs and enums remain eligible findings
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

- Planning: [`../../../logs/2026-06/04-230940-dead-code-data-shape-root-planning.md`](../../../logs/2026-06/04-230940-dead-code-data-shape-root-planning.md)
- Implementation: [`../../../logs/2026-06/04-231646-dead-code-data-shape-root-precision.md`](../../../logs/2026-06/04-231646-dead-code-data-shape-root-precision.md)

## Next Task

Planning checkpoint: decide the next residual dead-code batch under `g08.009`.
