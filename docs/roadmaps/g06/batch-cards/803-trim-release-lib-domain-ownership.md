# 803 - Trim Release Lib Domain Ownership

Roadmap: [`../003-release-domain-split-and-lib-reduction.md`](../003-release-domain-split-and-lib-reduction.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Start shrinking `crates/effigy-release/src/lib.rs` by pulling one durable
release-domain slice into its own owner module.

## Scope

- classify `effigy-release/src/lib.rs` by durable concept
- pick one high-confidence extraction slice
- keep the top-level release orchestration explicit
- preserve release safety, drift checks, and prepared-state behavior

## Acceptance

- `effigy-release/src/lib.rs` is materially smaller after the slice
- moved logic has a clearer owner module
- focused release tests stay green

## Completed

- Added [`crates/effigy-release/src/model.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-release/src/model.rs).
- Moved the release domain model and error surface out of
  `crates/effigy-release/src/lib.rs`.
- Re-exported the public model surface from the top-level release crate.
- Reduced `crates/effigy-release/src/lib.rs` from `1622` lines to `1314`.
- Removed `effigy-release/src/lib.rs` from the god-file warning set.
- Logged the slice in
  [`../../../logs/archive/2026-05/14-210000-release-model-owner-extraction.md`](../../../logs/archive/2026-05/14-210000-release-model-owner-extraction.md).

## Suggested Validation

```bash
cargo test release
cargo test --test cli_output_tests cli_release
effigy scan god-files --json
```

## Next Task

Execute `804`.
