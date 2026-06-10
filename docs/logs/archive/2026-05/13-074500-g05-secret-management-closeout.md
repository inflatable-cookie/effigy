# G05 Secret Management Closeout

Completed `721` and closed `g05`.

Final posture:

- `[secrets]` is the portable declaration surface for true secrets.
- `effigy-vault` is the supported local encrypted vault backend.
- task, container, Rhai, deploy, state, and artifact seams use declared secret
  targets.
- `secrets export --format env --output <PATH> --yes` exists only as an
  explicit plaintext compatibility bridge.
- `.env.schema` remains native validation/task-env compatibility, not the new
  secret authority.
- Varlock is deferred as a live backend adapter.

Validation:

- `cargo test secrets_tests`
- `cargo test task_env`
- `cargo test container_secret_env`
- `cargo test -p effigy-rhai secret`
- `cargo check -p effigy-env`
- docs path/contains checks
- `git diff --check`

Residual notes:

- Full `cargo test -p effigy-rhai` still has known unrelated failures outside
  the secret-filtered tests.
- Example App local vault initialisation remains an operator step before its
  declarations can be tightened to `required = true`.
