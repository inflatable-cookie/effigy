# 2026-03-11 10:17:18 - release wrapper delegation and contracts

## Summary
- Turned `scripts/check-release-gates.sh` into a compatibility wrapper over
  `effigy release gates`, with optional `effigy release verify-install` chaining
  when `--tag` is provided.
- Kept `scripts/check-release-install-from-tag.sh` as a thin compatibility
  wrapper over `effigy release verify-install`.
- Tightened the self-hosting release contract test so both wrapper scripts must
  stay executable and continue delegating to the built-in release surfaces.

## Why
- This reduces migration drift between the legacy shell entrypoints and the
  shipped `effigy release *` command family.
- It also moves the remaining `027` self-hosting work toward real old/new
  validation instead of maintaining duplicate release logic in parallel.

## Verification
- `cargo fmt --all`
- `cargo test --lib parse_release_ -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --lib current_repo_release_config_matches_self_hosting_release_surfaces -- --nocapture`
- `cargo test --test cli_output_tests cli_release_verify_install_ -- --nocapture`
- `bash -n scripts/check-release-gates.sh`
- `bash -n scripts/check-release-install-from-tag.sh`
- `cargo fmt --all -- --check`
- `git diff --check`
