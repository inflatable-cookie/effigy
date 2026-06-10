## 2026-03-11 13:07:00 - Release simulate version override

Batch: release-simulate-version-override

Context:
- `effigy release prepare --plan|--yes` already supported deliberate
  `--version <SEMVER>` overrides.
- `effigy release simulate` only reported suggested-versus-selected metadata but
  could not yet exercise a deliberate selected-version preview from the CLI.

Changes:
- Added `--version <SEMVER>` support to `effigy release simulate`.
- Reused the shared release-version validation path so simulate now enforces the
  same semver parsing, increment, and duplicate-version rules as prepare.
- Updated simulate text/JSON help and operator docs so the dry-run preview
  contract now includes deliberate selected-version previews.
- Added parse coverage plus end-to-end CLI tests for successful simulate
  override previewing and invalid simulate override rejection.

Verification:
- `cargo test --lib parse_release_ -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_simulate_ -- --nocapture`

Outcome:
- Movement: baseline `release simulate had passive version metadata only` ->
  current `release simulate can now preview an operator-selected valid version
  with the same no-write guarantees as its default dry-run path`
