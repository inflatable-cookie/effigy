# 804 - Converge Demo Release And Bootstrap Test Fixtures

Roadmap: [`../004-shared-fixture-and-test-support-convergence.md`](../004-shared-fixture-and-test-support-convergence.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Reduce the next chunk of repeated test scaffolding by moving shared fixture
builders behind clearer private support owners.

## Scope

- re-run duplicate-block evidence and classify the highest-value test-only
  setup duplication
- extract shared support only where two or more test families build the same
  fixture world
- keep assertion narratives local unless the assertion contract itself is also
  duplicated
- leave unrelated test domains alone

## Acceptance

- duplicate test setup shrinks materially
- concurrency-sensitive demo CLI tests share one support path
- release or bootstrap setup becomes visibly clearer
- focused test surfaces stay green

## Completed

- Added [`tests/shared/deploy_fixture_support.rs`](/Users/tom/Dev/projects/effigy/tests/shared/deploy_fixture_support.rs).
- Moved shared workspace-app bundle copying and deploy-provider fixture setup
  behind that support module.
- Reused the shared fixture owner from internal JSON-contract tests, runner
  deploy tests, and CLI JSON-envelope tests.
- Reduced duplicate-block findings from `96` to `93`.
- Reduced high duplicate-block findings from `8` to `6`.
- Logged the slice in
  [`../../../logs/2026-05/14-213500-shared-deploy-test-fixtures.md`](../../../logs/2026-05/14-213500-shared-deploy-test-fixtures.md).

## Suggested Validation

```bash
cargo test --test cli_output_tests
cargo test -p effigy-bootstrap
cargo test release
cargo run --bin effigy -- scan duplicate-blocks --json
```

## Next Task

Execute `805`.
