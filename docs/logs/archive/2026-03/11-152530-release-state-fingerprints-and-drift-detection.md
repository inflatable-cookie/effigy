## 2026-03-11 15:25:30 - Release state fingerprints and drift detection

Batch: release-state-fingerprints-and-drift-detection

Context:
- `effigy release resume` already provided a dedicated recovery surface for
  `.release-prepared.json`.
- Execute preflight already checked staleness and working-tree shape, but it
  still relied on raw file presence rather than proving the prepared branch,
  HEAD, and file contents were still the same sources that were reviewed at
  prepare time.

Changes:
- Extended `.release-prepared.json` to record source fingerprints for the
  prepared branch, prepared HEAD, and each prepared file digest.
- Taught execute preflight and resume summaries to compare current repository
  state against those fingerprints and surface branch drift, HEAD movement, and
  changed prepared-file contents as explicit blockers or drift items.
- Added compatibility handling for older prepared-state files that do not yet
  contain source fingerprints, with a warning that drift checks are limited in
  that case.
- Updated text and JSON release output so operators can see prepared branch,
  prepared HEAD, current HEAD, fingerprint availability, and the exact drift
  entries during `release execute --plan`, `release execute --yes`, and
  `release resume`.
- Added end-to-end CLI coverage for HEAD-plus-content drift in execute preflight
  and branch drift in release resume, plus state-file assertions proving the
  new fingerprints are persisted during prepare.

Verification:
- `cargo test --lib parse_release_ -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_plan_json_mode_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_resume_json_mode_ -- --nocapture`
- `cargo fmt --all`
- `cargo fmt --all -- --check`
- `git diff --check`

Outcome:
- Movement: `prepared release recovery only knew about stale time and working-tree shape`
  -> `prepared release recovery and execute preflight now also prove the reviewed
  branch, HEAD, and prepared file contents still match the recorded prepare state`
