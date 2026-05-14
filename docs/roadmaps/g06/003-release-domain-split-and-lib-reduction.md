# g06.003 - Release Domain Split And Lib Reduction

Status: Complete
Depends on: `g06.001`

## Goal

Shrink `crates/effigy-release/src/lib.rs` by splitting the release domain into
stable concepts instead of one central owner.

## Evidence

- `crates/effigy-release/src/lib.rs` remained a warning-level god file after
  `g05`
- release work now carries status, prepare, execute, simulate, git state,
  mutations, drift checks, stale checks, and prepared-state modeling
- release code is contract-heavy and test-heavy, so one large file increases
  review and regression burden

## Scope

- split release code into durable modules such as:
  state model, mutation plan, changelog/version source handling, execute
  preflight, git checks, and result rendering support
- preserve current crate API shape unless a cleaner internal module split is
  enough
- keep command-surface behavior identical
- add or move tests only where ownership becomes clearer

## Out Of Scope

- no release protocol redesign
- no workflow changes beyond those already explicitly approved
- no semantic change to stale/drift/retag safety rules
- no publish-artifact redesign

## Guardrails For A Cheaper Model

- split by release concepts, not by helper size alone
- do not hide safety checks behind opaque abstractions
- keep commit/tag/push diagnostics as readable as they are now
- preserve prepared-state file behavior exactly unless separately planned

## Suggested Implementation Steps

1. Inventory `lib.rs` by durable release concept.
2. Extract data models and mutation planning first.
3. Extract git/drift/preflight checks next.
4. Leave top-level orchestration thin and explicit.
5. Keep or improve current test readability during moves.

## Acceptance Criteria

- `effigy-release/src/lib.rs` is materially smaller
- release-domain ownership is visibly clearer
- release prepare/execute/status/simulate behavior remains stable
- retained monolithic areas are explicitly justified

## Validation

Minimum focused validation:

```bash
cargo test release
cargo test --test cli_output_tests cli_release
effigy scan god-files --json
```

## Current State

The first bounded release-domain split is landed:

- the core release model moved into
  `crates/effigy-release/src/model.rs`
- `crates/effigy-release/src/lib.rs` dropped below the god-file threshold
- release prepare, execute, status, simulate, and verify contract tests stayed
  green

Retained orchestration and release flow logic still lives in `lib.rs` by
design. It is no longer large enough to justify a dedicated follow-up lane on
size alone.

## Next Task

Continue with `g06.004`.
