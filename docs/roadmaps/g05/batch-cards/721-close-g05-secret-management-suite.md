# 721 - Close G05 Secret Management Suite

Roadmap: [`../007-varlock-adapter-and-closeout.md`](../007-varlock-adapter-and-closeout.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Close the `g05` secret and local configuration management generation.

## Scope

- confirm docs, help, command reference, JSON examples, and Rustdocs describe
  the final `g05` surface
- verify built-in vault, task/container/Rhai/deploy/state/artifact injection,
  compatibility export, and `.env.schema` posture are coherent
- confirm Underlay and Example App docs reference the final model
- run targeted secret-management validation
- update roadmap front doors to mark `g05` complete
- capture a closeout log

## Non-Goals

- no new feature implementation
- no Varlock adapter
- no provider-hosted secret provisioning
- no release commands
- no workflow edits

## Acceptance

- `g05` has no ready cards remaining.
- Public docs and help do not contradict the final secrets contract.
- Varlock is documented as deferred.
- `.env.schema` is documented as compatibility and validation, not the secret
  authority for new Effigy-managed projects.
- Validation results and any residual risks are recorded.

## Completed

- Reviewed secrets help, command reference, JSON examples, contract docs, and
  env-schema docs for the final `g05` posture.
- Fixed stale help/reference wording around the now-active local vault and
  plaintext export bridge.
- Confirmed Varlock is deferred and `.env.schema` remains compatibility and
  validation rather than the new secret authority.
- Confirmed Underlay and Example App docs now reference the Effigy-backed local
  vault model.
- Closed `g05` roadmap front doors with no ready cards remaining.

## Validation Notes

- `cargo test secrets_tests`
- `cargo test task_env`
- `cargo test container_secret_env`
- `cargo test -p effigy-rhai secret`
- `cargo check -p effigy-env`
- docs path/contains checks
- `git diff --check`

## Validation

- targeted secrets tests
- targeted Rhai/deploy/state/container redaction checks where feasible
- docs checks for touched docs
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

No next `g05` task after closeout.
