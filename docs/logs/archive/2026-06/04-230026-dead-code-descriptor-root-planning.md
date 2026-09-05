# Dead-Code Descriptor-Root Planning

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1047`

## Summary

Opened the next residual dead-code precision slice.

After trait/API surface handling, the residual scan is no longer dominated by
method findings. The clearest next false-positive class is descriptor or
dispatch table function ownership. `crates/effigy-cli/src/help/registry.rs`
still reports 27 private render wrapper functions even though each wrapper is
assigned to `HELP_TOPIC_DESCRIPTORS` and invoked through a function pointer
field.

## Baseline

Current `target/debug/effigy scan dead-code --json` baseline:

- findings: 521
- isolated files: 5
- unreferenced symbols: 516
- checked files: 798
- checked symbols: 2,524

Remaining symbol kinds:

- functions: 307
- structs: 174
- enums: 31
- methods: 2
- traits: 2

Largest path groups:

- `crates/effigy-cli/src/help/registry.rs`: 27
- `crates/effigy-release/src/render_json.rs`: 21
- `crates/effigy-runtime/src/data/volumes.rs`: 13
- `src/runner/deploy_command/transaction.rs`: 12
- `crates/effigy-tasks/src/listing.rs`: 12
- `crates/effigy-containers/src/policy_support/generated_compose.rs`: 12

## Decision

Make the next `g08.009` batch focus on descriptor and dispatch roots.

This should be a scanner/indexer precision fix, not a function allowlist. The
fix should credit functions assigned into descriptor or dispatch structures
while leaving unrelated private helpers visible. DTO/render/config structs and
enums should be classified as the next residual class, not hidden in this
slice.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: dead-code residuals are 521 findings after trait/API surface
  precision, with 307 function findings.
- Current: opened a ready card to reduce descriptor/dispatch function
  false positives without suppressing private helper functions.
- Remaining open: implement `1047`, rerun the self-scan, and choose the next
  residual precision slice from DTO/render models, associated-call matching,
  or real cleanup.

## Next Task

Run [`1047-classify-descriptor-and-dispatch-dead-code-roots.md`](../../roadmaps/g08/batch-cards/1047-classify-descriptor-and-dispatch-dead-code-roots.md).
