# g07.066 - Codegraph Test Harness Decomposition

Status: Planned
Depends on: `g07.065`

## Goal

Reduce `crates/effigy-codegraph/src/tests.rs` from a giant mixed proof surface
into smaller, intention-revealing test owners.

## Evidence

The current god-file scan still reports:

- `crates/effigy-codegraph/src/tests.rs`

The file mixes:

- extractor fixtures
- query behavior proof
- graph storage expectations
- parity/gold-query assertions
- helper builders and repeated repo setup paths

## Scope

- split graph tests by behavior family
- keep shared test helpers local to the codegraph crate
- improve failure locality so a broken query feature does not require reading a
  thousand-line mixed test file
- preserve current proof depth

## Guardrails

- no reduction in graph coverage just to lower line count
- no hidden fixture behavior behind opaque helper stacks
- no movement of graph tests into unrelated crates

## Suggested Implementation Shape

- move tests under `crates/effigy-codegraph/src/tests/`
- separate at least:
  - extractor coverage
  - query/search/context behavior
  - explore/parity cases
  - watch/index/status behavior
  - local test support

## Acceptance Criteria

- the main `tests.rs` owner disappears or becomes a thin facade
- graph test failures point at a clearer behavior family
- codegraph crate tests remain straightforward to run as a unit

## Next Task

After this lands, proceed to [`067-script-command-boundary-reduction.md`](./067-script-command-boundary-reduction.md).
