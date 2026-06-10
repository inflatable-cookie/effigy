# 805 - Converge CLI Help Topic Layout Machinery

Roadmap: [`../005-cli-help-and-rendering-deduplication.md`](../005-cli-help-and-rendering-deduplication.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Reduce the dominant remaining duplicate-block cluster in CLI help topic layout
without centralizing the topic content itself.

## Scope

- target repeated section framing and layout scaffolding only
- keep topic copy local to the owning help files
- preserve output order, spacing, and headings

## Acceptance

- duplicate-block findings drop in the CLI help cluster
- help topic ownership stays readable
- help output contracts remain stable

## Completed

- Added a shared `StandardTopicHelpSpec` render path in
  `crates/effigy-cli/src/help/topics/shared.rs`.
- Converted `bootstrap`, `docs`, `container`, and `release` help topics onto
  that shared layout path while keeping topic copy local.
- Reduced high duplicate-block findings from `6` to `4`.
- Logged the slice in
  [`../../../logs/archive/2026-05/14-220500-cli-help-layout-deduplication.md`](../../../logs/archive/2026-05/14-220500-cli-help-layout-deduplication.md).

## Suggested Validation

```bash
cargo test help
cargo test tasks_rendering
cargo run --bin effigy -- scan duplicate-blocks --json
```

## Next Task

Execute `806`.
