## 2026-03-11 12:46:00 - Release prepare version flag alignment

Implemented the next `g01.027` non-interactive alignment batch by adding
`--version <SEMVER>` to the non-interactive prepare paths.

### Delivered

- `effigy release prepare --plan --version <SEMVER>` now previews mutations for
  a deliberate version override instead of only the changelog-derived default.
- `effigy release prepare --yes --version <SEMVER>` now applies that deliberate
  override and writes suggested-versus-selected version metadata into
  `.release-prepared.json`.
- Plain interactive `effigy release prepare` rejects `--version` explicitly and
  points operators back to the built-in staged version review instead of
  silently double-driving version selection.

### Verification

- `cargo test --lib parse_release_prepare_ -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_plan_json_mode_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_yes_json_ -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
