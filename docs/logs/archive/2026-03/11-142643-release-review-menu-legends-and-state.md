## 2026-03-11 14:26:43 - Release review menu legends and state summaries

Batch: release-review-menu-legends-and-state

Context:
- Interactive release prepare/execute already used compact review menus with
  drill-down inspection.
- The remaining UX gap was operator context inside the menu itself: maintainers
  still had to remember the currently selected version or stale acknowledgement
  state and re-read the prompt footer to see the available commands.

Changes:
- Added persistent prepare-menu state summaries for suggested/selected version,
  planned tag, custom override state, mutation count, and gate review status.
- Added persistent execute-menu state summaries for prepared version/tag,
  stale acknowledgement state, readiness, and working-tree blocker counts.
- Added compact command legends and shortcut summaries directly inside both
  interactive review menus.
- Updated interactive CLI coverage to assert the new menu summaries and legend
  lines, including the stale acknowledgement transition from pending to
  recorded.
- Updated release help, roadmap, protocol, guide `051`, and changelog entries
  to describe the richer review-menu contract.

Verification:
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_interactive_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_ -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`

Outcome:
- Movement: `interactive release menus require prompt re-reading to recover
  state and commands` -> `interactive release menus keep the current state and
  command legend visible during review`
