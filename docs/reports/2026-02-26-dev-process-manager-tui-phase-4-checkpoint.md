# Dev Process Manager TUI Phase 4 Checkpoint

Date: 2026-02-26
Owner: Platform
Related roadmap: 004 - Dev Process Manager TUI

## Scope
- Promote managed `mode = "tui"` tasks to launch TUI by default on interactive terminals.
- Preserve non-interactive behavior with managed-plan output fallback.
- Stabilize managed stream output capture around process exit.
- Align roadmap status and documentation with implemented behavior.

## Changes
- Updated runner selection flow:
  - stream mode still honored via `EFFIGY_MANAGED_STREAM=1`,
  - TUI now auto-runs on interactive terminals,
  - non-interactive sessions render managed plan output.
- Added explicit override env handling:
  - `EFFIGY_MANAGED_TUI=0|false` disables TUI,
  - `EFFIGY_MANAGED_TUI=1|true` forces TUI.
- Fixed managed stream runtime race by draining late stdout/stderr events after process exits.
- Serialized env-sensitive managed-task tests for deterministic execution.
- Updated roadmap index and roadmap 004 checklist progress.
- Added guide `012-dev-process-manager-tui.md`.

## Validation
- command: `cargo test run_manifest_task_managed_`
  - result: pass (7 passed, 0 failed)

## Risks / Follow-ups
- Restart controls are not implemented yet.
- Exit summary and non-zero propagation behavior still need explicit policy.
- TUI keymap help overlay is not implemented yet.

## Next
- Implement phase 4.4 restart controls and exit-status propagation, then add integration coverage for restart and shutdown paths.
