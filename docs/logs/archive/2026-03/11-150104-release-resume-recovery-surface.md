## 2026-03-11 15:01:04 - Release resume recovery surface

Batch: release-resume-recovery-surface

Context:
- Effigy already supported prepare, execute preflight, and interactive execute
  review.
- The remaining recovery gap was a dedicated prepared-state entrypoint:
  operators had to infer the current `.release-prepared.json` state through
  `release execute` instead of using a purpose-built resume surface.

Changes:
- Added `effigy release resume` as a first-class release subcommand with
  parser, help, and runtime dispatch support.
- Added a dedicated resume summary renderer in text and JSON modes so operators
  can inspect prepared version/tag, prepared-at timestamp, stale state, drift,
  blockers, and suggested actions without immediately entering execute.
- Added a text-mode recovery menu that can inspect prepared state, inspect
  drift since prepare time, and hand off directly into the existing interactive
  execute review flow.
- Reused execute-plan drift analysis so stale-state and working-tree recovery
  uses the same source of truth as release execution.
- Added end-to-end CLI coverage for JSON recovery summaries and text-mode
  resume-to-execute handoff.

Verification:
- `cargo test --lib parse_release_ -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_resume_ -- --nocapture`

Outcome:
- Movement: `prepared release recovery required rediscovering state through
  execute` -> `prepared release recovery has a dedicated resume command with a
  summary view and execute-review handoff`
