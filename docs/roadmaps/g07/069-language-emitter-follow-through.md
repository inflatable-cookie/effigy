# g07.069 - Language Emitter Follow Through

Status: Planned
Depends on: `g07.068`

## Goal

Revisit the remaining high duplicate blocks across JS, PHP, and Python graph
emitters and reduce only the duplication that still represents real
maintenance risk.

## Evidence

The current duplicate-block scan still reports high findings across:

- `crates/effigy-codegraph/src/language/javascript.rs`
- `crates/effigy-codegraph/src/language/php.rs`
- `crates/effigy-codegraph/src/language/python.rs`

`g07.057` removed the first obvious boilerplate seam, but the remaining blocks
show there is still duplicated record-emission and diagnostic flow.

## Scope

- inspect each remaining high duplicate as either:
  - profitable shared helper
  - acceptable language-local duplication
  - wrong scan signal not worth chasing
- extract only helpers that preserve language-local semantics and readable call
  sites
- add focused extractor regression coverage if helper behavior changes

## Guardrails

- no generic multi-language extraction framework
- no shared helper that obscures IDs, provenance, or traversal meaning
- no optimization-only changes mixed into this lane

## Acceptance Criteria

- each remaining high emitter duplicate is removed or explicitly justified
- extractors stay readable and stable
- duplicate scan evidence is updated honestly

## Next Task

After this lands, proceed to [`070-runner-private-fixture-and-helper-convergence.md`](./070-runner-private-fixture-and-helper-convergence.md).
