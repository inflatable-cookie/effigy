# Doctor Roadmap 009 Closeout Validation

Date: 2026-02-28
Owner: Platform
Related roadmap: 009 - Doctor Health Consolidation

## Scope
- Close roadmap 009 by validating:
  - canonical `doctor` command surface,
  - `repo-pulse`/built-in `health` migration behavior,
  - `doctor --fix` safe baseline behavior,
  - JSON contract coverage (`--full`) after doctor/pulse consolidation.

## Changes
- Removed legacy pulse command surface internals and task modules.
- Kept explicit migration guidance for `repo-pulse`/built-in `health` requests.
- Implemented `doctor --fix` safe baseline:
  - `manifest.health_task_scaffold` applied when no `tasks.health` exists,
  - skipped fix reporting with reason when fix cannot be safely applied.
- Updated JSON contract index and checker for doctor schema and envelope-aware validation.
- Updated docs/guides/architecture references from pulse to doctor.

## Validation
- command: `cargo test --lib`
  - result: pass (`219` tests).
- command: `cargo test --test cli_output_tests`
  - result: pass (`37` tests).
- command: `./scripts/check-json-contracts.sh --full`
  - result: pass (all selected schemas validated, including `effigy.doctor.v1`, raw and envelope modes).
- command: `cargo run --quiet --bin effigy -- doctor --repo .`
  - result: exit `0`; doctor report rendered with root-resolution info + remediation findings.
- command: `cargo run --quiet --bin effigy -- doctor --fix --repo /private/tmp/effigy-doctor-fix-smoke-9wA6wK`
  - result: exit `0`; `manifest.health_task_scaffold` applied and reported; `tasks.health` scaffold written.
- command: `cargo run --quiet --bin effigy -- repo-pulse --repo .`
  - result: exit `1`; explicit migration guidance rendered: use `effigy doctor` or define `tasks.health`.

## Risks / Follow-ups
- `doctor --fix` currently covers a narrow safe allowlist (health task scaffold only).
- Future remediation expansion should keep explicit allowlist semantics and tests for skipped/unsafe paths.

## Next
- No remaining roadmap 009 work items; move follow-up remediation expansion to a new roadmap/backlog item if needed.
