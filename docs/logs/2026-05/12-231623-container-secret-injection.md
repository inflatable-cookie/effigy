# Container Secret Injection

Implemented batch card `717` for `g05.005`.

## Changed

- Resolved `[secrets.keys.*]` declarations with `targets = ["containers"]`
  before container startup.
- Blocked missing required container secrets before compose mutation.
- Passed resolved secret values into attached and detached compose startup
  through process environment overrides.
- Avoided generated plaintext secret files.
- Added tests for container secret resolution and missing required blockers.

## Validation

- `cargo test container_secret_env`
- `cargo check --all-targets`

## Next

Execute `718` to add explicit compatibility env export.
