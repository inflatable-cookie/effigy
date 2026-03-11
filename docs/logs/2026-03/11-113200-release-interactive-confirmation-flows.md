# 2026-03-11 11:32:00 - release interactive confirmation flows

## Summary
- Added text-mode interactive confirmation flows for plain
  `effigy release prepare` and `effigy release execute`.
- Interactive prepare now renders the prepare preview, prompts for approval,
  and then applies the prepare step. When release gates are configured, it
  auto-runs them by default.
- Interactive execute now renders the execute preflight, prompts for final
  approval, and then runs the existing commit/tag/push path.

## Why
- `027` still depended on `--plan` and `--yes` as the only shipped approval
  model, which left the main release commands themselves incomplete.
- This batch closes that core product gap without pretending the richer
  multi-step release wizard is already finished.

## Verification
- `cargo fmt --all`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_interactive_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_text_mode_confirms_and_runs -- --nocapture`
- `git diff --check`
