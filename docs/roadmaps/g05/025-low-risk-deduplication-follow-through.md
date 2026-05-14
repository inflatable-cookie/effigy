# g05.025 - Low-Risk Deduplication Follow-Through

Status: Planned
Depends on: `g05.013`, `g05.014`

## Goal

Reduce the high duplicate-block findings that are safe to address without
changing behavior.

Focus on CLI help topic shape and local test fixture builders.

## Evidence

- `effigy scan duplicate-blocks --json` reports 99 findings and 8 high findings
- high findings include repeated CLI help topic blocks across bootstrap,
  container, docs, and release topics
- high findings include duplicated bootstrap integration setup across
  `crates/effigy-bootstrap/tests/integration.rs` and
  `src/runner/bootstrap_command/tests.rs`
- high findings include duplicated release version-file tests across
  `crates/effigy-release/src/tests.rs` and
  `src/runner/release_command/tests.rs`
- duplicate temp repo helpers remain in container command tests

## Scope

- normalize CLI help topic descriptors where topic files repeat the same
  section shape
- keep help copy near the topic owner
- add private fixture builders for bootstrap tests where both domain and runner
  tests need the same setup
- add private fixture builders for release version-file tests
- reduce container temp-repo helper duplication if it is local and obvious
- rerun duplicate scan and record any retained warnings

## Out Of Scope

- no global test-support crate
- no rewrite of all tests
- no help content redesign
- no user-facing command behavior changes
- no broad formatter-only churn

## Guardrails For A Cheaper Model

- if a duplicate is test data that improves readability, leave it and document
  it
- do not centralize every help topic into one registry
- keep fixtures private to the nearest owner unless reuse is proven across
  multiple crates
- preserve exact help text unless a failing test proves the current text is
  wrong
- use duplicate scan as a guide, not as an absolute target

## Suggested Implementation Steps

1. Capture the current high duplicate findings.
2. Start with CLI help descriptors because they are low-risk.
3. Add bootstrap test fixture builders only for repeated setup blocks.
4. Add release version-file fixture/assertion helper.
5. Rerun focused tests for affected crates.
6. Rerun duplicate scan and write down retained duplicate categories.

## Acceptance Criteria

- high duplicate findings are reduced materially
- help output remains stable
- bootstrap and release tests remain clear and focused
- no new broad abstraction hides test intent
- retained duplicate blocks are explicitly justified

## Validation

Minimum focused validation:

```bash
cargo test -p effigy-cli help
cargo test -p effigy-bootstrap
cargo test release
effigy scan duplicate-blocks --json
```

## Next Task

After low-risk dedupe, move to `g05.026` to make the Rhai host surface easier to
maintain.
