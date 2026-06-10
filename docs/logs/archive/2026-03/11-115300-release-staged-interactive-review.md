## 2026-03-11 11:53:00 - Release staged interactive review

Implemented the next `g01.027` UX batch by expanding text-mode `effigy release prepare`
and `effigy release execute` from single confirmation prompts into staged review flows.

### Delivered

- `effigy release prepare` now walks through:
  - version review
  - per-file mutation review with before/after previews
  - gate-result review when gates are configured
  - final approval before writing `.release-prepared.json`
- `effigy release execute` now walks through:
  - prepared-state review
  - working-tree review
  - final approval before commit/tag/push
- Interactive cancellation messages now identify which review stage was declined.
- CLI tests now drive the staged input contract instead of a single `y/N` prompt.

### Verification

- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_interactive_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_text_mode_confirms_and_runs -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_yes_json_mode_supports_plain_version_file_and_shell_gate -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
