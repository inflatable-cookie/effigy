## 2026-03-11 12:54:00 - Release version rejection and simulate parity

Implemented the next `g01.027` safety/parity batch around prepare-version
overrides and simulation metadata.

### Delivered

- Added explicit CLI rejection coverage for invalid `release prepare --version`
  values and for non-incrementing overrides that do not advance past the
  current version.
- `effigy release simulate` now surfaces the same
  suggested-versus-selected version/tag metadata as prepare, even when no
  override is currently active.

### Verification

- `cargo test --test cli_output_tests cli_release_simulate_json_mode_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_plan_json_mode_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_yes_json_ -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
