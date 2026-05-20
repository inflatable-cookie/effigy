# g07.068 - High Duplicate Help Fragment Reduction

Status: Planned
Depends on: `g07.067`

## Goal

Remove the remaining high duplicate-block findings in CLI help topics without
making the topic files unreadable.

## Evidence

The current duplicate-block scan still reports high findings across:

- `crates/effigy-cli/src/help/topics/bootstrap.rs`
- `crates/effigy-cli/src/help/topics/demo.rs`
- `crates/effigy-cli/src/help/topics/container.rs`
- `crates/effigy-cli/src/help/topics/release.rs`

The residual duplication is mostly repeated option/example tables that already
live near shared help rendering helpers.

## Scope

- extract only repeated fragments that remain obviously readable
- keep topic-local wording and section ordering explicit
- add focused help render tests for touched topics

## Guardrails

- no universal mega-help schema
- no abstract helper that makes topic intent harder to read
- no wording churn without a concrete readability reason

## Acceptance Criteria

- the current high-severity help duplicates are reduced or eliminated
- topic files remain easy to read in isolation
- rendered help output stays pinned by tests

## Next Task

After this lands, proceed to [`069-language-emitter-follow-through.md`](./069-language-emitter-follow-through.md).
