## 2026-03-11 12:37:00 - Release prepare custom version override

Implemented the next `g01.027` workflow-quality batch by adding deliberate
custom-version selection to the staged interactive `effigy release prepare`
flow.

### Delivered

- Interactive prepare version review now accepts the suggested version or a
  custom semver override.
- Invalid custom versions are rejected with immediate feedback and the operator
  stays inside version review.
- Selected-versus-suggested version metadata now flows through:
  - prepare plan text/JSON
  - prepared result text/JSON
  - `.release-prepared.json`
- The selected tag now follows the chosen version instead of always using the
  changelog-derived default.

### Verification

- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --lib validate_prepare_version_override_rejects_non_incrementing_versions -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_interactive_ -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
