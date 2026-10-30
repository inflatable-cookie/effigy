# Live progress sites (Q-017)

Status: current for the v0.15 candidate. This closes the source-wide audit
that Q-017 required before the QA pin, and records the disposition of every
site that audit flagged, site by site. The rule itself lives in
[product guardrails](product-guardrails.md#human-facing-feedback); this page
is the site-by-site evidence.

The 2026-10-29 read-only audit
(`/Volumes/Builds/scratch/effigy/feedback-spinner-source-audit-20261029.md`)
was taken against `ee2c4e4c`. It listed 60 production call sites, of which 48
were candidates that needed a live-state check or a one-line disposition. The
tables below give one row per audited line: what that line was, where its job
lives now (with the current file and line), and its disposition.

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

`effigy lint:feedback` runs `scripts/check-feedback-inventory.py` and then a
Clippy sweep with `-D warnings`, both over `--workspace --all-targets`: a
crate compiled only as another package's dependency is never linted as a
package, so a `-p` list could miss a wrapper in it. The lint is not an optional
selector — `validate`, `qa` and `qa:gates` all run it, so the normal validation
path and the milestone QA path both fail on a reintroduced bare spinner.

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
  `Step::live_step`, `steps::finish_line`, and `exec::runtime_wait(label,
  awaiting)` for the runtime fallback. The old `Renderer::spinner`/
  `SpinnerHandle` API was unused and is deleted, so no crate can start a bare
  spinner of its own.

The same operation state reaches text, JSON and the TUI. A managed entry's
wait is one `effigy_tui::waits::EntryState`: the tab, the `--headless` text and
`status --json` render `EntryState::status_line()` and publish the same
machine-readable `waiting_on` (`kind`, and `name`/`phase`, `healthy`/`total`,
`probe` or delay) plus the scoped `waits` list, so no surface derives a label
of its own.

## Disposition by audited line

### Cache/volume CLI

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/cli/entrypoint.rs:258` | Transient `steps::step(transient_spinner_label(..))` around any cache/volume command | Removed from the entrypoint, which renders only the result; the command owns its wait at `crates/effigy-runtime/src/data.rs:98` (`CacheVolumeProgress::run`; inspect/prune/remove at 168, 217, 262), with per-volume sub-step and event | removed (entrypoint) / live (`effigy-runtime`) |

### Container up and integrations

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/container_command/lifecycle.rs:339` | `steps::during("registering routes", ..)` | `lifecycle.rs:839` — `integration_stage(progress, 5, "registering gateway routes")` on the integrations source | live |
| `src/runner/container_command/lifecycle.rs:356` | `steps::phase("settling the environment")` outer settling spinner | Removed: `lifecycle.rs:434` records that the settle adds no second fixed label, and the environment publishes its own wait (`crates/effigy-containers/src/environment/reconcile.rs:1172`) | removed |
| `src/runner/container_command/lifecycle.rs:409` | `steps::phase("preparing ports and loopback")` | `lifecycle.rs:503` — `TimedStep::live(preparation.clone())` | live |
| `src/runner/container_command/lifecycle.rs:424` | `steps::phase("rendering the stack")` rendering spinner | `lifecycle.rs:519` — `effigy_ui::steps::finish_line(true, "stack rendered from ..")` | immediate line |
| `src/runner/container_command/lifecycle.rs:449` | `steps::during("starting shared services", ..)` | `lifecycle.rs:554` — `steps::during` over the named shared-service source built at `lifecycle.rs:550-552` (the per-service subject comes from `support.rs`) | live |
| `src/runner/container_command/lifecycle.rs:491` | `steps::phase("starting the `<name>` stack")` whole-stack step | `lifecycle.rs:600` — `TimedStep::live(progress.clone())`, with per-service `runtime_wait` steps from `exec/compose_steps.rs` | live |

### Container readiness

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/container_command/readiness.rs:74` | Parent `TimedStep::new("waiting for services to report healthy")` | `readiness.rs:213-214` — parent `ProgressSource` ("the environment's services to report healthy") and `TimedStep::live`; per-service steps at 675 and 775 update from probes | live |

### Colima profile repair/reset

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/container_command/profile.rs:218` | `steps::step(label)` held for the recovery/reset | `profile.rs:245-247` — `ProfileProgress` builds a `ProgressSource` from the exact probe and a `live_step`; `phase` (256) and `observe` (263) move it | live |

### Environment gate and settle

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `crates/effigy-containers/src/environment/reconcile.rs:456` | `steps::step("waiting for environment `x` to be free")` | `reconcile.rs:640-641` — `ProgressSource::waiting_for("environment `x` to be free")` and `live_step`; the gate state is published at 286/365 | live |
| `crates/effigy-containers/src/environment/reconcile.rs:797` | `steps::step("settling environment `x`")` | `reconcile.rs:1172` — `phase("settling run <id>")` on the reconcile source (`phase` at 1197), so the frame names the run and its count | live |

### Doctor

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `crates/effigy-doctor/src/progress.rs:104` | `steps::step(label.active)` per check, fixed for the scan | `progress.rs:132-136` — `start_scan_with(label, count, event)` builds `ProgressSource::waiting_for(label.probe)` and a `live_step` | live |

### Bootstrap and deps setup

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/bootstrap_command/mod.rs:242` | `steps::step(&label)` per phase, fixed until the next event | `bootstrap_command/mod.rs:275` — `live_step(phase_progress(&label, awaiting))` per event, with the exact selector or checkout awaited | live |

### Deploy and provider packaging

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/deploy_command/provider_package.rs:251` | `steps::step(resolving_provider_label(..))` | `provider_package.rs:297-298` — source built from the exact git source and rev/ref; `live_step` at 262 | live |

### Release

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/release_command/ops.rs:188` | `root.set_label("release: continuing")` between gates | `ops.rs:206-211` — the root source takes the gate position/count and the last gate's outcome, then `waiting_on(NEXT_GATE_OR_STAGE)` (110-122) | live |
| `src/runner/release_command/ops.rs:329` | `steps::step(release_gate_label(..))` per gate | `ops.rs:368` — `live_step(gate_progress(position, total, gate, reused))`, source at 400, ending at 376-383 | live |

### Bundle sources

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/bundle_command.rs:380` | `steps::step(fetching_label(..))` per source | `bundle_command.rs:380-381` — `live_step(source_progress(source, subject, revision, local))`, source at 419 | live |

### Secrets

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/secrets_command.rs:177` | `steps::step("unlocking vault")` | `secrets_command.rs:240` — `SecretOpProgress::start(PROBE_VAULT_WRITE_GATE)` (start at 40-43) | live |
| `src/runner/secrets_command.rs:296` | `steps::step("running the vault generate hook")` | `secrets_command.rs:413` — `SecretOpProgress::start(PROBE_VAULT_GENERATE)` | live |
| `src/runner/execute/pipeline/standard.rs:827` | `steps::step("unlocking vault")` in the task pipeline | `standard.rs:868` — the same `SecretOpProgress` | live |

### Workspace and install

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/workspace_command/mod.rs:544` | `PlainRenderer::stderr(..).spinner(label)` handed one label | `workspace_command/mod.rs:568-569` — `WorkspaceOpProgress` builds the source and `live_step` (updates at 589); the renderer spinner API it used is deleted | live, old API removed |

### Task sequence

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/execute/sequence_run.rs:423` | `steps::step(sequence_step_label(..))` fixed for the step | `sequence_run.rs:474-475` — `live_step` over `sequence_step_progress` (608-610): the step's ordinal, the exact child step awaited and the elapsed wait | live |

### State apply and capture

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/state_command.rs:201` | `steps::step(action.label(&layer.key))` | `state_command.rs:209-210` — `live_step(progress.clone())` over the layer action source | live |
| `src/runner/state_command.rs:841` | `steps::step(&label)` for the capture action | `state_command.rs:451-460` — `capture_task_progress`/`capture_artifact_progress`; `live_step` at 927 | live |

### Gateway setup

Each call passes the exact object it awaits; the elevated child's own phase
lines move it (`observe_platform_phase`, `gateway_command/mod.rs:2157`).

| Audited site | Now | Disposition |
| --- | --- | --- |
| `src/runner/gateway_command/mod.rs:1853` (verifying the operator account) | `mod.rs:2346` | live |
| `src/runner/gateway_command/mod.rs:1864` (removing the platform setup) | `mod.rs:2359` | live |
| `src/runner/gateway_command/mod.rs:1868` (migrating root-owned gateway state) | `mod.rs:2368` | live |
| `src/runner/gateway_command/mod.rs:1874` (applying platform setup) | `mod.rs:2381` | live |
| `src/runner/gateway_command/mod.rs:1907` (removing the systemd user unit) | `mod.rs:2434` | live |
| `src/runner/gateway_command/mod.rs:1915` (stopping the gateway) | `mod.rs:2449` (observes the stop phases) | live |
| `src/runner/gateway_command/mod.rs:1924` (trusting mkcert's CA) | `mod.rs:2464` | live |
| `src/runner/gateway_command/mod.rs:1936` (platform setup with the admin prompt) | `mod.rs:2477` | live |
| `src/runner/gateway_command/mod.rs:1945` (removing the setup receipt) | `mod.rs:2510` | live |
| `src/runner/gateway_command/mod.rs:1951` (recording the setup receipt) | `mod.rs:2518` | live |
| `src/runner/gateway_command/mod.rs:1959` (installing the systemd user unit) | `mod.rs:2528` | live |
| `src/runner/gateway_command/mod.rs:1975` (probing the low-port capability) | `mod.rs:2548` | live |
| `src/runner/gateway_command/mod.rs:1994` (starting the gateway as the operator) | `mod.rs:2576` (observes the start phases) | live |

### Gateway up, down and status

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `src/runner/gateway_command/mod.rs:548` | `live("starting the gateway")` + one `set_label` | `mod.rs:633` — `live_progress("starting the gateway process", "the gateway to answer on <addr>")` | live |
| `src/runner/gateway_command/mod.rs:574` | `live("reading gateway state")` | `mod.rs:668` — `live_progress("reading the gateway record and process", "the gateway identity record at <path>")` | live |
| `src/runner/gateway_command/mod.rs:589` | `live("replacing the running gateway")` | `mod.rs:692` — `live_progress("stopping the gateway on pid <pid>", "gateway process <pid> to exit")` | live |
| `src/runner/gateway_command/mod.rs:598` | `live("starting the gateway")` + one `set_label` | `mod.rs:706` — `live_progress("starting the gateway daemon", "the gateway daemon process to start")` | live |
| `src/runner/gateway_command/mod.rs:625` | `live("stopping the gateway")` | `mod.rs:785` — `live_progress("stopping the gateway process", "the gateway process to exit and its record at <path> to clear")` | live |
| `src/runner/gateway_command/mod.rs:644` | `live("reading gateway state")` | `mod.rs:812` — `live_progress("reading the gateway record and process", ..)` | live |
| `src/runner/gateway_command/mod.rs:662` | `live("stopping the gateway")` | `mod.rs:841` — `live_progress("stopping the gateway process on pid <pid>", ..)` | live |
| `src/runner/gateway_command/mod.rs:692` | `live("removing the gateway record")` | `mod.rs:871` — `live_progress("removing the gateway identity record", "the gateway identity record at <path> to be removed")` | live |
| `src/runner/gateway_command/mod.rs:850` | `live("reading gateway state")` with no update at all | `mod.rs:1042` — `live_progress("reading the gateway state", "the gateway process and its record at <path>")` | live |
| `src/runner/gateway_command/mod.rs:877` | `live("reading gateway state")` + one `set_label` | `mod.rs:1076` — `live_progress("checking the gateway process and its identity record", ..)` | live |
| new site (not in the audit) | — | `mod.rs:755` — `live_progress("reading the route table", "the route table at <path>")` | live |

### TUI panes and shutdown

| Audited site | What it was | Now | Disposition |
| --- | --- | --- | --- |
| `crates/effigy-tui/src/multiprocess/render/panes/output.rs:67` | Generic `waiting_label` under a moving glyph, aged from the session clock | Managed entries project `EntryState::status_line()` (`output.rs:34-44`, `names_its_wait` at 72); the fallback names the phase and its own elapsed wait (`output.rs:48-60`, `startup_meta_line` at 92, label at 122). The clock belongs to the process's current attempt (`state/startup.rs:12-24`; the view model passes it at `view_model/mod.rs:47`), so a queued entry cannot show its queue time as starting time | live |
| `crates/effigy-tui/src/multiprocess/lifecycle/progress.rs:23` | Canned `&'static str` per shutdown stage | `lifecycle/progress.rs:25-48` — each supervisor stage callback redraws its own stage, and the completion names the real `total`/`forced` counts; no animated glyph | live |

## Paths the audit had already cleared

These keep their disposition under the gate, with current evidence.

| Audited site | Now | Disposition |
| --- | --- | --- |
| `crates/effigy-containers/src/health.rs:49` | `health.rs:170-181` — source from the attempt/check, per-probe events | live |
| `crates/effigy-containers/src/exec/compose_steps.rs:30` | `compose_steps.rs:142` — `runtime_wait(event.label(..), "the runtime's next report")` per service/image move (helper at 101) | live |
| `src/runner/container_command/readiness.rs:260` | `readiness.rs:772-777` — per-service sub-step and last event | live |
| `src/runner/task_engine/admission.rs:591` | `admission.rs:600` — `live_step(waiting)` where `waiting` names the hook's decision probe and takes the hook's progress line | live |
| `src/runner/task_engine/admission.rs:530` | `admission.rs:531-539` — `finish_line(false, ..)` for the refused protocol read | immediate line |
| `src/runner/task_engine/admission.rs:548` | `admission.rs:552-560` — `finish_line(false, ..)` for the durable-run custody refusal | immediate line |
| `src/runner/release_command/ops.rs:117` | `ops.rs:120-122` — the run's source names the action and `NEXT_GATE_OR_STAGE` | live |
| `src/runner/bootstrap_command/mod.rs:257` | `bootstrap_command/mod.rs:283-291` — finished phases are `finish`/`note` one-liners | immediate line |
| `src/runner/release_command/ops.rs:338` | `ops.rs:376-383` — a gate that never started ends with `finish_line(result.passed, ..)` | immediate line |
| `src/runner/managed_run/tui.rs` | Still suspends outer terminal steps while the TUI owns the terminal; the pane projectors above own nested behavior | no step |
| `crates/effigy-manifest/src/bundles/source/progress.rs` | Supplies the `SourceProgress` Started/Finished events the bundle command projects (`bundle_command.rs:380`); it constructs no step | no step |
| `crates/effigy-ui` and TUI spinner constructors | One sink remains (`steps.rs` `start_bar`); the renderer's `spinner` path is deleted | removed |

## A new site

A step that can wait starts from `ProgressSource::waiting_for(the exact thing
awaited)` and calls `update`/`event`/`now_waiting_for` as the operation moves.
If the operation truly cannot report more, the frame still names that exact
wait and the elapsed time. A short operation that ends immediately calls
`finish_line`. `effigy lint:feedback` rejects anything else, in every workspace
crate.
