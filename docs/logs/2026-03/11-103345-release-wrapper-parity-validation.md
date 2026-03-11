# 2026-03-11 10:33:45 - release wrapper parity validation

## Summary
- Added end-to-end wrapper parity tests for the remaining self-hosting release
  migration surfaces.
- The no-tag `scripts/check-release-gates.sh` path now has proof that it runs
  the built-in `effigy release gates` flow on an Effigy-shaped fixture repo.
- The tagged `scripts/check-release-install-from-tag.sh` path now has proof
  that it runs the built-in `effigy release verify-install` flow on a tagged
  git fixture.

## Why
- Section 8 of roadmap `027` had already moved the legacy scripts to
  wrapper-only behavior, but it still lacked real old/new parity evidence.
- This batch closes that gap so the remaining self-hosting work can focus on
  adoption, checklist/workflow updates, and eventual wrapper retirement instead
  of questioning whether the shipped command mappings are correct.

## Verification
- `cargo fmt --all`
- `cargo test --test cli_output_tests cli_release_gate_wrapper_matches_builtin_no_tag_path -- --nocapture`
- `cargo test --test cli_output_tests cli_release_verify_install_wrapper_matches_builtin_tagged_path -- --nocapture`
- `cargo test --lib current_repo_release_config_matches_self_hosting_release_surfaces -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
