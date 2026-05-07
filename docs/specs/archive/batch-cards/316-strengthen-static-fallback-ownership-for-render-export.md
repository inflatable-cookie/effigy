# 316 Strengthen Static Fallback Ownership For Render Export

Status: landed
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Promote static-site fallback and rewrite ownership into `deploy.model.v1` so
the first Render exporter can generate static-site routes without guessing.

## In Scope

- extend the neutral deployment model with explicit static fallback metadata
- update the Underlay derivation contract and example
- derive the new field for the shipped Underlay static services
- add tests that pin the widened model honestly

## Out Of Scope

- Render exporter implementation
- Railway planning or implementation
- Decodelabs derivation
- broader static-hosting provider policy

## Acceptance Criteria

- the neutral model no longer leaves SPA fallback ownership implicit
- Underlay static services derive explicit fallback metadata
- the Render contract no longer has to block on static fallback ambiguity
- tests prove the widened field through the live `deploy model --json` path

## Validation

- `cargo test -p effigy runner::tests::runner_core_tests::deploy_tests --lib -- --nocapture`
- `cargo test --lib json_contract_tests::deploy_contract_tests -- --nocapture`
- `cargo test --test cli_output_tests cli_json_mode_deploy_model_wraps_deploy_payload -- --nocapture`
- `./target/debug/effigy docs check-paths docs/contracts/002-production-deployment-model.md docs/contracts/003-underlay-deployment-derivation.md docs/contracts/004-underlay-reference-deploy-model-example.md docs/contracts/007-render-export-contract.md docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/batch-cards/README.md docs/specs/batch-cards/316-strengthen-static-fallback-ownership-for-render-export.md`

## Next Task

After `316`, execute:

- [`317-implement-render-export-foundation.md`](./317-implement-render-export-foundation.md)
