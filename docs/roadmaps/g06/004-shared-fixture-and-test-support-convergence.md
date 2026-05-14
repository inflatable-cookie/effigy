# g06.004 - Shared Fixture And Test Support Convergence

Status: Complete
Depends on: `g06.001`

## Goal

Reduce repeated test scaffolding by giving demo, bootstrap, release, and other
high-churn test surfaces clear shared fixture owners.

## Evidence

- `g05.025` already removed some bootstrap/release duplication, but duplicate
  findings remain
- recent concurrent-runner demo test failures showed that lifecycle-sensitive
  tests need shared locking and fixture support instead of ad hoc ownership
- repeated temp repo setup, fake manifests, active-attempt records, and release
  fixture builders increase both LOC and flake risk

## Scope

- converge repeated fixture builders where two or more test families shape the
  same test world
- centralize concurrency-sensitive demo test support
- centralize release/bootstrap/container temp workspace helpers only where
  reuse is real
- reduce repeated assertion helpers when they defend the same contract shape

## Out Of Scope

- no giant test-support crate
- no broad rewrite of every test file
- no reduction in behavioral coverage
- no mixing unrelated domains into one fixture module

## Guardrails For A Cheaper Model

- prefer per-domain private support modules over one global shared junk drawer
- move fixture builders, not whole test narratives
- do not dedupe assertions if that makes failures harder to read
- keep tests obviously coupled to the contract they verify

## Suggested Implementation Steps

1. Re-run duplicate-block scan and classify test-only findings.
2. Identify fixture/setup duplication with the highest maintenance cost.
3. Extract private per-domain support modules.
4. Migrate the most unstable or repeated tests first.
5. Re-scan duplication after each meaningful batch.

## Acceptance Criteria

- duplicate test scaffolding is materially reduced
- demo CLI concurrency-sensitive tests share one serialization/fixture path
- release/bootstrap fixture ownership is clearer
- test failure messages remain readable

## Validation

Minimum focused validation:

```bash
cargo test --test cli_output_tests
cargo test -p effigy-bootstrap
cargo test release
effigy scan duplicate-blocks --json
```

## Current State

The first shared-fixture convergence slice is landed:

- deploy-provider fixture setup is shared across JSON-contract, runner, and
  CLI JSON-envelope tests
- the workspace-app bundle copy helper is no longer re-owned in those three
  surfaces
- the previous high-severity deploy-fixture duplicate cluster is gone

Remaining warning-level narrative duplication in individual test bodies is
acceptable for now. The next dominant duplicate target is the CLI help topic
layout cluster.

## Next Task

Continue with `g06.005`.
