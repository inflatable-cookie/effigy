## 2026-03-11 15:02:00 - Release reviewed markers and remediation hints

Batch: release-reviewed-markers-and-remediation-hints

Context:
- Interactive release prepare/execute menus already showed command legends and
  current selection state.
- The remaining operator assistance gap was progress tracking and blocked-output
  clarity: menus did not remember what had already been reviewed, and blocked
  prepare/execute output only listed raw blockers without pointing at the next
  likely fix.

Changes:
- Added in-session reviewed-section markers to the interactive prepare and
  execute menus, covering version/stale, mutation/state, gate/working-tree,
  and final preview sections.
- Kept those markers persistent across non-linear menu navigation so operators
  can jump around without losing track of what was already inspected.
- Added shared remediation-hint rendering for blocked release prepare and
  execute output, including changelog, gate, stale-state, working-tree, and git
  setup guidance.
- Extended interactive and text-mode CLI coverage to assert reviewed markers in
  the menus and suggested actions in blocked prepare/execute output.
- Hardened the execute test bare-remote helper against temp-path collisions
  during parallel test runs.

Verification:
- `cargo test --lib review_menu_renderers_show_review_markers -- --nocapture`
- `cargo test --lib remediation_hints_cover_prepare_and_execute_blockers -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_ -- --nocapture`
- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`

Outcome:
- Movement: `interactive review requires remembering what was already checked
  and blocked output only reports raw blockers` -> `interactive review tracks
  reviewed sections in-place and blocked output points at likely remediation
  actions`
