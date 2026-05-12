# 676 - Extract Deploy Report Models And History Helpers

Roadmap: [`../037-deploy-domain-boundary-hardening.md`](../037-deploy-domain-boundary-hardening.md)
Strict lane: [`../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md`](../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md)
Contract: [`../../../contracts/029-deploy-domain-boundary-contract.md`](../../../contracts/029-deploy-domain-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move deploy transaction report structs and report path/history helpers out of
`transaction.rs`.

## Scope

- extract plan/apply/status/history/redeploy report structs
- extract schema constants
- extract active/latest/history path helpers
- extract report path bundle helper
- extract safe path component helper if needed by report paths
- preserve JSON field names and skip rules exactly
- keep command orchestration in `transaction.rs`

## Non-Goals

- no provider package dispatch changes
- no deploy config parser changes
- no text rendering split yet
- no JSON schema changes
- no command behavior changes

## Acceptance

- `transaction.rs` no longer owns deploy report struct definitions
- `transaction.rs` no longer owns active/latest/history path construction
- deploy plan/apply/status/history/redeploy JSON output is unchanged
- deploy transaction tests still pass

## Outcome

- added `src/runner/deploy_command/report.rs`
- moved deploy plan/apply/status/history/redeploy report structs into the new
  report owner
- moved deploy schema constants into the report owner
- moved active/latest/history path helpers and JSON report read/write helpers
  into the report owner
- kept deploy command orchestration in `transaction.rs`
- reduced `transaction.rs` from 1,256 lines to 951 lines

## Validation

```sh
cargo test run_deploy_plan
cargo test run_deploy_apply
cargo test run_deploy_status
cargo test run_deploy_history
cargo test run_deploy_redeploy
cargo check --bin effigy
git diff --check
```

## Next Task

Execute `677` to isolate provider-package dispatch context.
