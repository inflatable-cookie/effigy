# 675 - Classify Deploy Transaction Ownership

Roadmap: [`../037-deploy-domain-boundary-hardening.md`](../037-deploy-domain-boundary-hardening.md)
Strict lane: [`../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md`](../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md)
Contract: [`../../../contracts/029-deploy-domain-boundary-contract.md`](../../../contracts/029-deploy-domain-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Map current deploy transaction ownership before moving deploy code.

## Scope

- classify functions and models in `transaction.rs`
- classify provider package resolution and phase dispatch in
  `provider_package.rs`
- classify static deploy model/export ownership in `model.rs`, `derive.rs`,
  `render.rs`, and `railway.rs`
- identify which tests prove each deploy surface
- decide the first implementation slice for `676`
- update contract `029` if the discovered owner map differs from the intended
  shape

## Non-Goals

- no code movement
- no command behavior changes
- no JSON schema changes
- no provider package behavior changes

## Suggested Evidence Commands

```sh
wc -l src/runner/deploy_command/*.rs
rg -n "^(pub |pub\\(|fn |struct |enum |impl )" src/runner/deploy_command/*.rs
rg -n "deploy plan|deploy apply|deploy status|deploy history|deploy redeploy|effigy.deploy" src/tests docs/contracts docs/guides
```

## Acceptance

- deploy transaction owner map is recorded
- relevant test coverage is recorded
- first implementation slice is selected
- `676` can execute without a fresh planning pass

## Outcome

- recorded the current deploy ownership map in contract `029`
- recorded runner and JSON contract test coverage in contract `029`
- selected deploy report models plus active/latest/history helpers as the first
  implementation slice
- kept provider-package dispatch out of the first split to reduce drift risk

## Validation

- docs review
- `git diff --check`

## Next Task

Execute `676` with the selected first split.
