# Dead-Code Data-Shape Root Planning

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1048`

## Summary

Opened the next residual dead-code precision batch inside `g08.009`.

After descriptor/dispatch root handling, the largest remaining groups are no
longer callable wrappers. They are mostly DTO, render, config, and generated
policy data shapes. The next batch should reduce that false-positive class
without hiding all private structs or enums.

## Baseline

Current `target/debug/effigy scan dead-code --json` baseline:

- findings: 488
- isolated files: 5
- unreferenced symbols: 483
- checked files: 798
- checked symbols: 2,498

Remaining symbol kinds:

- functions: 274
- structs: 174
- enums: 31
- methods: 2
- traits: 2

Largest path groups:

- `crates/effigy-release/src/render_json.rs`: 21
- `crates/effigy-runtime/src/data/volumes.rs`: 13
- `crates/effigy-tasks/src/listing.rs`: 12
- `crates/effigy-containers/src/policy_support/generated_compose.rs`: 12
- `src/runner/deploy_command/transaction.rs`: 11
- `src/runner/deploy_command/derive.rs`: 11

## Decision

Make `1048` focus on DTO/render/config data-shape roots.

This remains one batch under `g08.009`, not a new roadmap. The implementation
should credit data-shape roots from source or graph evidence and keep unused
private structs and enums visible.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: dead-code residuals are 488 findings after descriptor/dispatch
  root handling, including 174 struct and 31 enum findings.
- Current: opened a ready card to reduce DTO/render/config data-shape false
  positives without blanket type suppression.
- Remaining open: implement `1048`, rerun the self-scan, and choose the next
  residual batch from associated-call matching, isolated-file inspection, or
  real cleanup.

## Next Task

Run [`1048-classify-dto-render-config-dead-code-roots.md`](../../roadmaps/g08/batch-cards/1048-classify-dto-render-config-dead-code-roots.md).
