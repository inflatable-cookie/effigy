## 2026-03-11 15:36:11 - Release recovery shortcuts for drift

Batch: release-recovery-shortcuts-for-drift

Context:
- `effigy release resume` and `effigy release execute` already surfaced stale,
  working-tree, and source-fingerprint drift clearly.
- The remaining resilience gap was operator recovery inside those flows:
  maintainers still had to leave the interactive review path to rerun gates,
  discard old prepared state, or start a clean prepare after semantic drift.

Changes:
- Added direct `gates`, `reprepare`, and `discard` shortcuts to the interactive
  `release resume` recovery menu.
- Added the same `gates`, `reprepare`, and `discard` shortcuts to the
  interactive `release execute` review menu.
- Extended blocked execute preflight review so those shortcuts are available
  directly from the blocked browser, not only after backing out to the main
  execute menu.
- Implemented `reprepare` as a real recovery handoff: it confirms removal of
  the existing `.release-prepared.json`, discards it, and then enters the
  interactive prepare flow instead of failing immediately on the existing state
  file.
- Added a dedicated prepared-state discard confirmation/result path so recovery
  can clear `.release-prepared.json` explicitly without pretending to revert
  any working-tree changes.
- Updated help, guides, roadmap notes, and remediation hints so operators see
  these recovery actions as part of the built-in release workflow.

Verification:
- `cargo test --lib review_menu_renderers_show_review_markers -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_resume_interactive_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_ -- --nocapture`
- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`

Outcome:
- Movement: `semantic drift reports blockers but operators still have to leave the flow to recover`
  -> `interactive release recovery now has built-in gate reruns, discard-state cleanup,
  and clean reprepare handoff shortcuts directly from resume, execute, and blocked preflight review`
