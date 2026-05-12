# Low-Risk Deduplication Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/038-docs-policy-cli-help-and-test-fixture-deduplication.md`](../roadmaps/g04/038-docs-policy-cli-help-and-test-fixture-deduplication.md)
Strict lane: [`../specs/074-low-risk-deduplication-strict-lane.md`](../specs/074-low-risk-deduplication-strict-lane.md)
Status: Accepted
Owner: Platform
Updated: 2026-05-12

## Purpose

Define the safe boundary for removing high-confidence duplication discovered by
the post-v0.6.x codebase sweep.

This lane is structural. It should reduce drift risk without changing command
behavior, JSON contracts, help semantics, provider behavior, or release posture.

## Hard Boundaries

- no public command grammar changes
- no JSON schema changes
- no help text redesign
- no release execution
- no `.github/workflows/` edits
- no broad parser rewrite
- no new public test-support crate unless at least two production crates need
  the same stable boundary
- no abstractions that hide the behavior under test

## Ownership Rules

Domain behavior tests belong with the domain crate.

Runner tests should prove adaptation:

- command-to-domain invocation
- error mapping
- report mapping
- side-effect orchestration
- text/JSON boundary behavior

CLI help cleanup should prefer readable data normalization over macros. Help
topic source must remain reviewable without expanding generated code mentally.

Fixture cleanup should prefer private builders near the tests that use them.
Public APIs must not grow only to make tests shorter.

## First Targets

The first accepted targets are:

- duplicate docs-policy tests between `crates/effigy-docs-policy/src/tests.rs`
  and `src/runner/docs_command/tests.rs`
- repeated help-topic data shapes in `crates/effigy-cli/src/help/topics/`
- repeated container/runtime policy fixtures in tests and runner support

## Acceptance Boundary

This contract is satisfied when:

- docs-policy behavior is tested once in `effigy-docs-policy`
- docs runner tests cover runner-owned adaptation only
- duplicate-block critical findings for docs-policy tests are removed
- CLI help duplication is reduced or explicitly deferred with rationale
- fixture builders are introduced only where they improve ownership clarity
- validation proves no command/runtime behavior drift

## Accepted Shape

The accepted `g04.038` cleanup shape is:

- docs-policy behavior tests live in `effigy-docs-policy`
- docs command runner tests cover runner adaptation only
- common CLI help option rows live in `topics::shared`
- literal-heavy help topic arrays stay explicit for source review
- runner-local `EffectiveContainerPolicy` fixtures use
  `runner::test_support::effective_container_policy`
- public test-support crates remain deferred until a real production-adjacent
  ownership boundary exists

The final duplicate scan for this lane reported `critical=0 high=7 warning=84
findings=91`. Remaining high findings are deliberately deferred to future
domain-test ownership and help-system review cards.
