# 2026-03-11 11:05:00 - release cross-project adoption

## Summary
- Added end-to-end release CLI coverage for non-Cargo repo shapes:
  `package.json`, `pyproject.toml`, and plain `VERSION`.
- Verified that release gates can run as generic shell commands on those repo
  types rather than assuming Rust-specific tooling.
- Added release orchestration examples to the agent adoption guide for Node.js,
  Python, and multi-language/plain-version repos.

## Why
- Section 9 of roadmap `027` was still open even though the core release code
  already supported multiple version-file formats.
- This batch closes the adoption proof gap with operator-facing tests and
  examples instead of leaving cross-project support as an unverified claim.

## Verification
- `cargo fmt --all`
- `cargo test --test cli_output_tests cli_release_status_json_mode_supports_package_json_and_shell_gates -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_plan_json_mode_supports_pyproject_auto_detection -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_yes_json_mode_supports_plain_version_file_and_shell_gate -- --nocapture`
- `git diff --check`
