# 683 - Add Container Runtime Test Fixture Builders

Roadmap: [`../038-docs-policy-cli-help-and-test-fixture-deduplication.md`](../038-docs-policy-cli-help-and-test-fixture-deduplication.md)
Strict lane: [`../../../specs/074-low-risk-deduplication-strict-lane.md`](../../../specs/074-low-risk-deduplication-strict-lane.md)
Contract: [`../../../contracts/030-low-risk-deduplication-contract.md`](../../../contracts/030-low-risk-deduplication-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Centralize repeated container/runtime test fixture setup where builders make the
tests clearer.

## Scope

- inspect repeated `EffectiveContainerPolicy` and service fixture literals
- add private builders close to the tests that use them
- avoid public test-support APIs unless a stable ownership boundary exists
- preserve behavior and assertions

## Non-Goals

- no container runtime behavior changes
- no public crate boundary changes
- no broad fixture framework

## Acceptance

- repeated fixture literals are reduced where safe
- affected container/runtime/runner tests pass
- duplicate-block scan shows improvement or records intentional deferral

## Outcome

- added a private runner test helper for `EffectiveContainerPolicy` fixtures
- moved `exec_command`, `host_container_lease`, and workspace command tests to
  the shared runner fixture helper
- kept cross-crate container/runtime fixture duplication out of scope because a
  public test-support API would be premature

## Validation

- `cargo test exec_command::tests`
- `cargo test host_container_lease::tests`
- `cargo test system_command::workspace::tests`
- `cargo check --bin effigy`
- `cargo fmt --all -- --check`
- `effigy scan duplicate-blocks --json`
- `git diff --check`

## Next Task

Execute `684` to close the duplicate scan proof.
