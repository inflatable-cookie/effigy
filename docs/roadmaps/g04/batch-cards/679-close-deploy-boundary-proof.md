# 679 - Close Deploy Boundary Proof

Roadmap: [`../037-deploy-domain-boundary-hardening.md`](../037-deploy-domain-boundary-hardening.md)
Strict lane: [`../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md`](../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md)
Contract: [`../../../contracts/029-deploy-domain-boundary-contract.md`](../../../contracts/029-deploy-domain-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close the deploy domain boundary hardening lane after proving deploy behavior
stayed stable.

## Scope

- record final module shape
- mark roadmap `037` complete
- mark strict lane `073` complete
- mark contract `029` accepted
- advance front doors to the next g04 roadmap item

## Outcome

- `transaction.rs` remains the deploy transaction orchestration owner
- `report.rs` owns deploy report schemas, report paths, JSON report
  persistence, and JSON conversion helpers
- `provider_context.rs` owns provider package context JSON assembly and package
  policy blocker mapping
- `text.rs` owns human-facing deploy text rendering
- provider package execution remains in `provider_package.rs`
- static deploy model/export behavior remains unchanged
- `transaction.rs` reduced from 1,256 lines to 856 lines

## Validation

```sh
cargo test deploy_tests::
cargo check --bin effigy
git diff --check
```

All listed validation passed.

## Next Task

Execute `g04.038` for docs-policy, CLI help, and fixture deduplication.
