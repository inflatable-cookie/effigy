# Dead-Code Trait-Surface Planning

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1046`

## Summary

Opened the next residual dead-code precision slice.

The broad test-scope question is resolved: Rust `#[test]` functions are
entrypoints, but unused helpers under tests remain visible. The next highest
value class is trait/API method surface handling, because the scanner still
reports method and trait symbols that are likely reachable through trait,
backend, renderer, or scan option interfaces.

## Baseline

Current `target/debug/effigy scan dead-code --json` baseline:

- findings: 661
- isolated files: 5
- unreferenced symbols: 656
- checked files: 798
- checked symbols: 2,660

Remaining symbol kinds:

- functions: 358
- structs: 173
- methods: 92
- enums: 31
- traits: 2

Largest path groups:

- `crates/effigy-cli/src/help/registry.rs`: 27
- `crates/effigy-builtin/src/scan/execution/core/api.rs`: 23
- `crates/effigy-release/src/render_json.rs`: 21
- `crates/effigy-ui/src/renderer.rs`: 16
- `crates/effigy-builtin/src/ports.rs`: 15
- `crates/effigy-cli/src/help/mod.rs`: 13
- `crates/effigy-runtime/src/data/volumes.rs`: 13
- `crates/effigy-containers/src/policy_support/generated_compose.rs`: 12
- `crates/effigy-tasks/src/listing.rs`: 12
- `src/runner/deploy_command/transaction.rs`: 12

## Decision

Make the next `g08.009` batch focus on trait and API surface precision.

This is a scanner precision fix, not a suppression policy. The work should
classify trait declarations, trait impl methods, public API surface methods,
and private inherent methods separately. Trait/API false positives can be
reduced, but private unused inherent methods and unused test helpers must
remain visible.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: dead-code residuals are down to 661 after test-entrypoint handling,
  but method/trait findings remain too mixed for cleanup use.
- Current: opened a ready card to reduce trait/API surface false positives
  without hiding private method findings.
- Remaining open: implement `1046`, rerun the self-scan, and choose the next
  residual precision slice from descriptor/dispatch roots, associated-call
  matching, DTO/render models, or real cleanup.

## Next Task

Run [`1046-classify-trait-and-api-surface-dead-code.md`](../../roadmaps/g08/batch-cards/1046-classify-trait-and-api-surface-dead-code.md).
