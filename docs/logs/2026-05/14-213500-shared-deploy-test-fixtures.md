# Shared Deploy Test Fixtures

Date: 2026-05-14
Roadmap: `g06.004`
Batch card: `804`

## Summary

Converged the deploy-provider test fixture world behind one shared private
support module reused by internal and integration test surfaces.

## Changes

- added [`tests/shared/deploy_fixture_support.rs`](/Users/tom/Dev/projects/effigy/tests/shared/deploy_fixture_support.rs)
- moved shared workspace-app bundle copying into the shared fixture owner
- moved shared deploy-provider fixture writing into the same owner
- rewired:
  - [`src/tests/json_contract_tests/prelude.rs`](/Users/tom/Dev/projects/effigy/src/tests/json_contract_tests/prelude.rs)
  - [`src/tests/runner_tests/prelude/fixtures.rs`](/Users/tom/Dev/projects/effigy/src/tests/runner_tests/prelude/fixtures.rs)
  - [`src/tests/runner_tests/runner_core_tests/deploy_tests.rs`](/Users/tom/Dev/projects/effigy/src/tests/runner_tests/runner_core_tests/deploy_tests.rs)
  - [`tests/cli_output_tests/json_envelope_tests/mod.rs`](/Users/tom/Dev/projects/effigy/tests/cli_output_tests/json_envelope_tests/mod.rs)
  - related call sites in deploy JSON-contract and CLI-runtime tests

## Outcome

- duplicate-block findings dropped from `96` to `93`
- high duplicate-block findings dropped from `8` to `6`
- the previous high-severity deploy fixture duplication cluster is gone
- retained overlap in this area is now warning-level test narrative setup, not
  shared fixture-owner duplication

## Validation

- `cargo test deploy_contract_tests`
- `cargo test run_deploy_`
- `cargo test --test cli_output_tests cli_json_mode_deploy_`
- `cargo run --bin effigy -- scan duplicate-blocks --json`
