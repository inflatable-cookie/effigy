# 684 - Close Duplication Scan Proof

Roadmap: [`../038-docs-policy-cli-help-and-test-fixture-deduplication.md`](../038-docs-policy-cli-help-and-test-fixture-deduplication.md)
Strict lane: [`../../../specs/074-low-risk-deduplication-strict-lane.md`](../../../specs/074-low-risk-deduplication-strict-lane.md)
Contract: [`../../../contracts/030-low-risk-deduplication-contract.md`](../../../contracts/030-low-risk-deduplication-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close `g04.038` with evidence from tests and duplicate-block scan output.

## Scope

- run focused tests for touched areas
- run `effigy scan duplicate-blocks --json`
- record remaining high findings that are intentionally deferred
- update contract, strict lane, and roadmap status

## Non-Goals

- no new cleanup beyond the accepted lane scope
- no release execution

## Acceptance

- `g04.038` is complete or has explicit deferred findings
- duplicate-block critical/high counts are recorded
- next task advances to `g04.039`

## Outcome

- duplicate scan improved from `critical=2 high=7 warning=87 findings=96` to
  `critical=0 high=7 warning=84 findings=91`
- docs-policy duplicate behavior tests were removed from the runner layer
- common help option rows were centralized without hiding topic content
- runner-local container policy fixtures were centralized where safe

## Deferred Findings

- CLI help topic array findings remain high because the scanner normalizes
  different literal-heavy usage/options/example arrays into similar token
  shapes; replacing them with heavier generated descriptors would reduce scan
  noise but make help content harder to review.
- bootstrap and release domain/runner duplicated tests remain high and should be
  handled in a later domain-test ownership card, not hidden inside this
  container/runtime fixture slice.
- cross-crate container/runtime policy fixture literals remain warnings because
  introducing a public test-support crate only for fixture line count would be
  the wrong abstraction boundary.

## Validation

- `cargo test -p effigy-docs-policy`
- `cargo test docs_command`
- `cargo test -p effigy-cli`
- `cargo test exec_command::tests`
- `cargo test host_container_lease::tests`
- `cargo test system_command::workspace::tests`
- `cargo check --bin effigy`
- `cargo fmt --all -- --check`
- `effigy scan duplicate-blocks --json`
- `git diff --check`

## Next Task

Execute `g04.039` to review artifact internals and crate boundaries.
