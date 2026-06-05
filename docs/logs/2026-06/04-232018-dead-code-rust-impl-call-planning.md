# Dead-Code Rust Impl-Call Planning

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1049`

## Summary

Opened the next residual dead-code precision batch inside `g08.009`.

After data-shape root handling, residual findings are mostly functions. The
largest groups point at Rust impl/call precision rather than deletion: renderer
trait impl methods, graph storage helper methods, and config-doc section helper
functions all look like places where source relationships are still
under-credited.

## Baseline

Current `target/debug/effigy scan dead-code --json` baseline:

- findings: 285
- isolated files: 5
- unreferenced symbols: 280
- checked files: 798
- checked symbols: 2,293

Remaining symbol kinds:

- functions: 274
- structs: 2
- methods: 2
- traits: 2
- enums: 0

Largest path groups:

- `crates/effigy-ui/src/plain_renderer/mod.rs`: 10
- `crates/effigy-codegraph/src/storage.rs`: 10
- `crates/effigy-manifest/src/bundles.rs`: 8
- `crates/effigy-changelog/src/parser.rs`: 8
- `crates/effigy-builtin/src/config/docs/tasks.rs`: 8
- `src/runner/secrets_command.rs`: 7
- `crates/effigy-scan/src/render/graph.rs`: 7

## Decision

Make `1049` focus on Rust impl and associated-call precision.

This is still scanner/indexer precision work. Do not start deleting function
pockets until impl method roots and associated/self call references are credited
well enough that remaining findings are more likely to be real cleanup.

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: dead-code residuals are 285 findings after data-shape root
  handling, with 274 function findings.
- Current: opened a ready card to reduce Rust impl/call false positives without
  blanket function suppression.
- Remaining open: implement `1049`, rerun the self-scan, then decide whether
  the next batch should be real cleanup, isolated-file inspection, or another
  narrow graph precision class.

## Next Task

Run [`1049-classify-rust-impl-and-associated-call-dead-code.md`](../../roadmaps/g08/batch-cards/1049-classify-rust-impl-and-associated-call-dead-code.md).
