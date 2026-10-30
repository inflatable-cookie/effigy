# Live progress sites (Q-017)

Status: current for the v0.15 candidate. This closes the source-wide audit
that Q-017 required before the QA pin, and records the disposition of every
site that audit flagged. The rule itself lives in
[product guardrails](product-guardrails.md#human-facing-feedback); this page
is the site-by-site evidence.

The 2026-10-29 read-only audit
(`/Volumes/Builds/scratch/effigy/feedback-spinner-source-audit-20261029.md`,
against `ee2c4e4c`) listed 60 production call sites, of which 48 were
candidates that needed a live-state check or a one-line disposition. Its line
numbers have moved with the migrations below; the table names each site by
plane and file so it can still be found.

Every site has one of three dispositions:

- **live**: a `ProgressSource` drives the step, so its frames name the current
  sub-step or object, a known count, the last event and the elapsed wait, or
  the exact awaited probe when the operation cannot report more. Each plane
  proves its frames on a fake clock.
- **immediate line**: the operation ends in the same breath, so it is one
  `finish_line` (or `note`) with no spinner at all.
- **removed**: the construct no longer exists.

There is no fourth disposition. A site that cannot name a real live-state
source is a missing hook for its owning plane, not an exception.

## The gate

The same operation state reaches text, JSON and the TUI. A managed entry's
wait is one `effigy_tui::waits::EntryState`: the tab, the `--headless` text and
`status --json` render `EntryState::status_line()` and publish the same
machine-readable `waiting_on` (`kind`, and `name`/`phase`, `healthy`/`total`,
`probe` or delay) plus the scoped `waits` list, so no surface derives a label
of its own.

`effigy lint:feedback` runs `scripts/check-feedback-inventory.py` and then a
Clippy sweep with `-D warnings`.

- The legacy fixed-label entry points (`steps::step`, `Step::step`,
  `TimedStep::new`, `exec::phase`) are deprecated anchors. rustc resolves a use
  through aliases and wrappers before it emits the diagnostic, so the check
  reports the real call site with the file, the line and the migration.
- There is no allowlist, ceiling or baseline file. Any such use fails, and so
  does an `#[allow(deprecated)]`-style suppression.
- `config/feedback-lint/clippy.toml` disallows
  `indicatif::ProgressBar::new_spinner` everywhere except the one reviewed
  sink in `crates/effigy-ui/src/steps.rs`, so a new helper that builds its own
  bare spinner fails even before it is wired to a label.
- The shipped API is the gate as well: `steps::live_step(ProgressSource)`,
  `Step::live_step`, `steps::finish_line`, and `exec::runtime_wait` for the
  runtime fallback. The old `Renderer::spinner`/`SpinnerHandle` API was unused
  and is deleted, so no crate can start a bare spinner of its own.

## Disposition by audit row

| Audit site (2026-10-29) | Live source today | Disposition |
| --- | --- | --- |
| Cache/volume CLI `src/cli/entrypoint.rs:258` | `crates/effigy-runtime/src/data.rs` `CacheVolumeProgress` (`ProgressSource::waiting_for("runtime volume inspect")`, per-volume sub-step and event) | live |
| Container up/integrations `lifecycle.rs:339,356,409,424,449,491` | `src/runner/container_command/lifecycle.rs` integration-stage source (`INTEGRATION_STAGES`, gateway port wait observer, primary-container probe), `steps.rs` `TimedStep::live`, `support.rs` shared-service wait, `exec/compose_steps.rs` per-service `runtime_wait` | live |
| Container readiness parent `readiness.rs:74` | `readiness.rs` parent `ProgressSource` ("the environment's services to report healthy") plus per-service sources and exited-log reads | live |
| Colima repair/reset `profile.rs:218` | `profile.rs` `ProfileProgress` over a `ProgressSource` with `profile_observation` events | live |
| Environment gate/settle `reconcile.rs:456,797` | `crates/effigy-containers/src/environment/reconcile.rs` gate source (`publish_gate_state`) and `ReconcileProgress::phase` with counts | live |
| Doctor `progress.rs:104` | `crates/effigy-doctor/src/progress.rs` `start_scan_with(label, count, event)`; each check has its own source and probe | live |
| Bootstrap/deps setup `bootstrap_command/mod.rs:242` | `bootstrap_command/mod.rs` per-event `BootstrapProgressReporter` phases over `phase_progress`; `note` for finished phases | live |
| Deploy/package `provider_package.rs:251` | `provider_package.rs` resolving-provider source (offline rev or online ref, exact git source) | live |
| Release `ops.rs:188,329` | `release_command/ops.rs` root and per-gate sources (`position/total`, gate name, exact command or hosted evidence) | live |
| Bundle sync/prepare `bundle_command.rs:380` | `bundle_command.rs` `source_progress` per source (source, subject, revision/ref) | live |
| Secrets `secrets_command.rs:177,296`, `pipeline/standard.rs:827` | `secrets_command.rs` `SecretOpProgress` (probe, sub-step, count, event; no secret value) used by the vault and pipeline paths | live |
| Workspace/install `workspace_command/mod.rs:544` | `workspace_command/mod.rs` `WorkspaceOpProgress`; the renderer spinner it used is deleted | live, and the old API removed |
| Task sequence `sequence_run.rs:423` | `execute/sequence_run.rs` `sequence_step_progress` (label, ordinal, exact child step awaited, elapsed) | live |
| State apply/capture `state_command.rs:201,841` | `state_command.rs` per-layer, per-hook and per-artifact sources with counts and events | live |
| Gateway setup `gateway_command/mod.rs:1853..1994` | `gateway_command/mod.rs` `setup_step` over a source, plus `observe_platform_phase` from the elevated child's own phase lines | live |
| Gateway up/down/status `gateway_command/mod.rs:548..877` | `gateway_command/mod.rs` `live_progress`/`live_step` with start/stop/probe observers | live |
| TUI panes `render/panes/output.rs:67` | entry-state projection (`EntryState::status_line`, `names_its_wait`); the generic fallback now shows the startup phase and the elapsed wait | live |
| TUI shutdown `lifecycle/progress.rs:23` | per-stage supervisor callback with the completion's real counts; no animated glyph | live |

## Paths the audit had already cleared

These keep their disposition under the gate:

- `crates/effigy-containers/src/health.rs` (attempt/check events) - live.
- `crates/effigy-containers/src/exec/compose_steps.rs` (service/image events) -
  live.
- `src/runner/task_engine/admission.rs` (hook progress line becomes the
  sub-step; the frame always names the hook's decision probe and the elapsed
  wait) - live.
- `src/runner/release_command/ops.rs:117` root stage/gate updates - live.
- Immediate outcomes in `src/runner/bootstrap_command/mod.rs`,
  `src/runner/release_command/ops.rs` and
  `src/runner/task_engine/admission.rs` are `finish_line`/`note` calls, not
  steps - immediate line.
- `src/runner/managed_run/tui.rs` still suspends outer terminal steps while the
  TUI owns the terminal; the pane projectors above own the nested behavior.

## A new site

A step that can wait starts from `ProgressSource::waiting_for(the exact thing
awaited)` and calls `update`/`event`/`now_waiting_for` as the operation moves.
If the operation truly cannot report more, the frame still names that exact
wait and the elapsed time. A short operation that ends immediately calls
`finish_line`. `effigy lint:feedback` rejects anything else.
