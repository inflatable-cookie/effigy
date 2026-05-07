# 313 Strengthen Deploy-Model Production Metadata Foundation

Status: landed
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Strengthen `deploy.model.v1` enough that provider adapters can stay thin.

## In Scope

- settle the neutral-model shape for the next known production metadata seams:
  - static-service output ownership
  - release-hook promotion
  - health-probe promotion
- update the deployment contracts and example model where needed
- extend the Underlay derivation path to emit the new fields or the sharper
  warnings those fields require
- keep the surface Underlay-only and JSON-only
- add tests that pin the new model shape honestly

## Out Of Scope

- Render export files
- Railway export files
- live provisioning
- Decodelabs derivation

## Acceptance Criteria

- the contract docs no longer leave the main provider-facing model gaps
  unresolved
- `deploy.model.v1` carries the settled production metadata or the explicit
  warning boundary for each seam
- Underlay derivation emits the strengthened shape deterministically
- tests prove the new fields or warning semantics through the live command path

## Validation

- `cargo test -p effigy runner::tests::runner_core_tests::deploy_tests --lib -- --nocapture`
- `cargo test --lib json_contract_tests::deploy_contract_tests -- --nocapture`
- `cargo test --test cli_output_tests cli_json_mode_deploy_model_wraps_deploy_payload -- --nocapture`
- `cargo run --quiet --bin effigy -- docs check-paths docs/contracts/002-production-deployment-model.md docs/contracts/003-underlay-deployment-derivation.md docs/contracts/004-underlay-reference-deploy-model-example.md`

## Next Task

After `313`, the widening decision moves to:

- [`314-decide-post-production-metadata-widening.md`](./314-decide-post-production-metadata-widening.md)
