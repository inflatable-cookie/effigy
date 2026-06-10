## 2026-03-11 12:15:00 - Release stale-state acknowledgement

Implemented the next `g01.027` operator-safety batch by tightening execute-time
handling for stale `.release-prepared.json` state.

### Delivered

- `effigy release execute --plan` now treats stale prepared state as blocked by
  default and reports that explicit `--allow-stale` is required.
- `effigy release execute --yes` now also requires `--allow-stale` before it
  will execute against stale prepared state.
- Plain text-mode `effigy release execute` now inserts a dedicated stale-state
  acknowledgement step before the prepared-state / working-tree / final-review
  sequence.
- Execute plan and result payloads now report whether stale override was
  required and whether it was used.

### Verification

- `cargo test --lib parse_release_execute_ -- --nocapture`
- `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_ -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
