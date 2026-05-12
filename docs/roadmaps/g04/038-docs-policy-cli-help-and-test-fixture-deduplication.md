# 038 - Docs Policy, CLI Help, And Test Fixture Deduplication

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`037-deploy-domain-boundary-hardening.md`](./037-deploy-domain-boundary-hardening.md)

## Goal

Remove high-confidence duplication that increases drift without changing user
behavior.

## Evidence

- duplicate scan reported 158 duplicated lines between
  `crates/effigy-docs-policy/src/tests.rs` and
  `src/runner/docs_command/tests.rs`
- duplicate scan reported repeated CLI help-topic blocks across bootstrap,
  container, docs, and release help topics
- duplicate scan reported repeated container policy and service fixture shapes
  across container, runtime, runner, and system tests

## Scope

- keep docs policy behavior tests in `effigy-docs-policy`
- reduce runner docs tests to command adaptation and error/report behavior
- consider a typed CLI help topic descriptor if it reduces duplication without
  hiding content
- add private test-support builders where repeated fixtures are local to a crate
- remove duplicated fixture literals that obscure the behavior under test

## Non-Goals

- no public help text redesign
- no docs command behavior changes
- no public test-support crate unless multiple production crates truly need it
- no snapshot-test churn without clear value
- no broad rewrite of CLI parsing

## Core Decisions

### Docs Policy Tests

Domain behavior belongs in `effigy-docs-policy`. Runner tests should prove that
the runner invokes policy checks and renders results correctly.

### CLI Help Topics

Prefer data normalization over clever macros. Help text should stay readable in
source review.

### Test Fixtures

Prefer private builders close to the tests that use them. Do not leak test-only
builders into public APIs just to reduce line count.

## Acceptance Criteria

- docs-policy duplicate blocks are removed
- runner docs tests still cover command adaptation
- CLI help topic duplication is deliberately documented as acceptable where the
  scanner is matching literal-heavy arrays with different user-facing content
- repeated runner-local container policy fixture setup is centralized where safe
- duplicate-block scan shows reduced critical findings and fewer total findings

## Outcome

- reduced duplicate scan from `critical=2 high=7 warning=87 findings=96` to
  `critical=0 high=7 warning=84 findings=91`
- consolidated docs-policy test ownership into the domain crate
- added shared common option rows for CLI help topics
- added private runner fixture support for repeated container policy literals
- deferred cross-crate fixture support and broader help-system changes until a
  stronger ownership boundary exists

## Suggested Batch Cards

- `680-open-low-risk-deduplication-lane.md`
- `681-consolidate-docs-policy-test-ownership.md`
- `682-normalize-cli-help-topic-data-shape.md`
- `683-add-container-runtime-test-fixture-builders.md`
- `684-close-duplication-scan-proof.md`

## Validation

- `effigy-docs-policy` tests
- CLI help tests
- container/runtime runner tests touched by fixture builders
- `effigy scan duplicate-blocks --json`
- `git diff --check`

## Next Task

Execute `g04.039` to review artifact internals and crate boundaries.
