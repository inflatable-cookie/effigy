# Code ratings at `a9b3213f9`

The tables `C1.csv`–`C5.csv` hold 707 product-code modules (C1 154, C2 63,
C3 97, C4 179, C5 214), rated at commit `a9b3213f9` with columns
`id,files,loc,given,corrected,source,action,reason,body_sha`. Scores are frozen:
the re-rate is a separate step that writes new tables, never edits to these.
Column semantics are in `README.md`.

## Calibration (why the scores are corrected)

Independent Claude Sonnet sub-raters re-scored 60 modules blind (12 per domain,
including the 5 largest by churn × size):

| Group | Mean difference (blind − given) |
|---|---|
| C1 | −1.50 |
| C2 | −1.67 |
| C3 | −1.58 |
| C4 | −1.08 |
| C5 | −1.92 |
| Files of 1,500+ lines | −1.92 |
| Smaller files | −1.29 |
| Overall (mean absolute difference 1.62) | −1.55 |

Deepseek was lenient mainly on Proportion (size and patch layers) and Typed facts.
Correction: sampled modules take their blind score; all others are offset by −1.9
(1,500+ lines) or −1.3 (smaller). The ranking within each domain is preserved.

## Corrected distribution

- **Mean:** 7.89, down from a given 8.96.
- **Scores:**

| Score | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|
| Modules | 4 | 11 | 64 | 137 | 260 | 229 | 2 |

- **5 or below:** 15 modules (2%), about 24k lines.
- **6 or below:** 79 modules (11%), about 83k lines.
- **7 or below:** 216 modules (31%), about 174k lines.

Lowest: deploy_command, workspace provisioning, container support, script_command (4);
container lifecycle, host_process, host_process::kernel_run, container_runtime_prep,
execute/pipeline/standard (5).

## Concrete defects the calibration found (verify first)

- `inline_workspace.rs`: the mount-escape check is lexical, so a nonexistent `../x` source passes.
- `container_runtime_prep/mod.rs`: swallows errors and relaxes runtime data to 0777.
- `environment/reconcile.rs`: discards settle results when admission fails.
- `container_command/lifecycle.rs`: classifies failures by matching an error-message prefix.
- `host_process`: string status and dropped results (`let _`, `Err(_) => {}`).
- `managed::run_spec`: renders a typed run to a shell string, then re-parses it.
- `managed_run::engine`: fails open in `pending_stop` when the request read fails.
- `execute/pipeline`: parses `"exit="` from strings, discards a vault read error, and has three near-copies of delivered-env resolution.
- Duplicate helpers across crates: `parse_mount_parts` (four copies), `shell_quote`, hex/sha helpers.

## Module identity (how `id` and `files` were normalised)

Each frozen rating row (a mix of Rust paths such as `runner::deploy_command` and
file paths) became a stable `id` plus the exact file set it covered at
`a9b3213f9`. Single-file rows list that file; directory modules list the
facade `.rs` (when the rater counted it) plus the group's files, minus files
claimed by a nested rated row and minus test-only files (`tests/` trees,
`tests.rs`, `test_support`). Every row's file count and every group row's line
total were verified arithmetically against the frozen `files`/`loc` columns.

Deliberate exceptions, all verified by exact file-count and line-total match:

- Three rated overlaps are preserved as rated: the `events` row includes its
  nested `key_command` row's four product files; `test/render` shares
  `results.rs` with the `test/render/results` row; `src/runner/execute/pipeline`
  covers all five files including the separately-rowed `managed.rs` and
  `standard.rs` trees. Drift in a shared file flags every row that rated it.
- Two frozen `loc` values do not equal their whole-file sums and are copied
  verbatim: `test/planning/resolve` (rated 1140, files sum to 1103) and
  `src/runner/execute/pipeline` (rated 578; only the entry files sum near it).
  Drift uses `body_sha`, never `loc`.
- Rater gaps (product files at `a9b3213f9` with no row, now reported as new):
  `test/planning.rs`, `test/planning/runnable/cargo_env.rs`, `arg_parser.rs`
  is folded into its group row, and the `waits.rs` re-export shim is folded
  into the `multiprocess/mod.rs` row.

## Target options (Tom decides)

| Option | Target | Work |
|---|---|---|
| A, floor 6 (recommended first) | No module below 6, and every concrete defect above fixed | 15 modules (about 24k lines), mostly splits of the big hotspots, plus the defect list |
| B, floor 7 | No module below 7 | 79 modules (about 83k lines); a long programme |
| C | Mean 8.5 | Most of the codebase |

Round 5 works bottom-up, with smaller batches than for tests, and needs native
proof for runtime paths. It runs after round 4 has brought tests to floor 7.

## Drift on main: the re-rate work list

`effigy qa:quality:code -- --summary` on main (2026-11-13):

```text
code quality: 707 rows, 94 new, 144 changed, 2 gone, 30.0% stale since a9b3213f9
```

Full report: `effigy qa:quality:code`. Changed rows need their rating
re-verified against the new text; gone rows and new files need fresh rows;
split groups are the expected Round 5 splits. Scores stay frozen until the
re-rate lands.

### Gone rows (2): re-rate as new module structure

- `runner::deploy_command` (missing: `deploy_command/derive.rs`, `model.rs`,
  `provider_context.rs`, `provider_package.rs`, `transaction.rs`)
- `runner::script_command` (missing: `script_command/feature_dispatch.rs`)

### Split: re-rate groups (2)

- `runner::deploy_command` → `deploy_command/code_ref.rs`, `export.rs`,
  `inputs.rs`, `model/` (derive, manifest, merge, mod, production, vocabulary),
  `plan.rs`, `provider/` (context, descriptor, mod, phase, report, source),
  `refuse.rs`, `status.rs`
- `runner::script_command` → `script_command/feature_dispatch/` (container,
  deploy, docs, inspect, mod, state, surface), `nested_error.rs`

### New product files with no row (94)

Grouped by directory; the two `test/planning` files are rater gaps from the
frozen table, the rest is post-`a9b3213f9` growth and Round 5 splits.

#### crates/effigy-builtin/src (2)

- `crates/effigy-builtin/src/test/planning.rs`
- `crates/effigy-builtin/src/test/planning/runnable/cargo_env.rs`

#### crates/effigy-containers/src (1)

- `crates/effigy-containers/src/exec/engine.rs`

#### crates/effigy-gateway/src (1)

- `crates/effigy-gateway/src/proxy/performance.rs`

#### crates/effigy-kernel/src (8)

- `crates/effigy-kernel/src/fault.rs`
- `crates/effigy-kernel/src/fault_injection.rs`
- `crates/effigy-kernel/src/faults/driver.rs`
- `crates/effigy-kernel/src/faults/matrix.rs`
- `crates/effigy-kernel/src/faults/mod.rs`
- `crates/effigy-kernel/src/faults/oracles.rs`
- `crates/effigy-kernel/src/faults/scenarios.rs`
- `crates/effigy-kernel/src/faults/world.rs`

#### src/runner (3)

- `src/runner/container_runtime.rs`
- `src/runner/execute.rs`
- `src/runner/task_secrets.rs`

#### src/runner/container_command (21)

- `src/runner/container_command/lifecycle/down.rs`
- `src/runner/container_command/lifecycle/exec.rs`
- `src/runner/container_command/lifecycle/integrations.rs`
- `src/runner/container_command/lifecycle/logs.rs`
- `src/runner/container_command/lifecycle/prepare.rs`
- `src/runner/container_command/lifecycle/report.rs`
- `src/runner/container_command/lifecycle/reset.rs`
- `src/runner/container_command/lifecycle/shell.rs`
- `src/runner/container_command/lifecycle/stats.rs`
- `src/runner/container_command/lifecycle/status.rs`
- `src/runner/container_command/lifecycle/up.rs`
- `src/runner/container_command/support/container_readiness.rs`
- `src/runner/container_command/support/gateway_aliases.rs`
- `src/runner/container_command/support/host_ports.rs`
- `src/runner/container_command/support/repo_scope.rs`
- `src/runner/container_command/support/report_annotations.rs`
- `src/runner/container_command/support/runtime_commands.rs`
- `src/runner/container_command/support/runtime_identity.rs`
- `src/runner/container_command/support/shared_compose.rs`
- `src/runner/container_command/support/shared_services.rs`
- `src/runner/container_command/support/workspace_preparation.rs`

#### src/runner/container_runtime_prep (10)

- `src/runner/container_runtime_prep/activation.rs`
- `src/runner/container_runtime_prep/binding.rs`
- `src/runner/container_runtime_prep/cargo_inputs.rs`
- `src/runner/container_runtime_prep/checkout_mount.rs`
- `src/runner/container_runtime_prep/compose_hash.rs`
- `src/runner/container_runtime_prep/exec_prep.rs`
- `src/runner/container_runtime_prep/host_mounts.rs`
- `src/runner/container_runtime_prep/image_platform.rs`
- `src/runner/container_runtime_prep/predecessor.rs`
- `src/runner/container_runtime_prep/sealed_render.rs`

#### src/runner/deploy_command (18)

- `src/runner/deploy_command/code_ref.rs`
- `src/runner/deploy_command/export.rs`
- `src/runner/deploy_command/inputs.rs`
- `src/runner/deploy_command/model/derive.rs`
- `src/runner/deploy_command/model/manifest.rs`
- `src/runner/deploy_command/model/merge.rs`
- `src/runner/deploy_command/model/mod.rs`
- `src/runner/deploy_command/model/production.rs`
- `src/runner/deploy_command/model/vocabulary.rs`
- `src/runner/deploy_command/plan.rs`
- `src/runner/deploy_command/provider/context.rs`
- `src/runner/deploy_command/provider/descriptor.rs`
- `src/runner/deploy_command/provider/mod.rs`
- `src/runner/deploy_command/provider/phase.rs`
- `src/runner/deploy_command/provider/report.rs`
- `src/runner/deploy_command/provider/source.rs`
- `src/runner/deploy_command/refuse.rs`
- `src/runner/deploy_command/status.rs`

#### src/runner/execute (3)

- `src/runner/execute/nested.rs`
- `src/runner/execute/planning.rs`
- `src/runner/execute/render.rs`

#### src/runner/host_process (9)

- `src/runner/host_process/configuration.rs`
- `src/runner/host_process/dependencies.rs`
- `src/runner/host_process/kernel_run/activation.rs`
- `src/runner/host_process/kernel_run/endpoints.rs`
- `src/runner/host_process/kernel_run/observation.rs`
- `src/runner/host_process/kernel_run/stopping.rs`
- `src/runner/host_process/listener.rs`
- `src/runner/host_process/listener_wait.rs`
- `src/runner/host_process/supervision.rs`

#### src/runner/script_command (8)

- `src/runner/script_command/feature_dispatch/container.rs`
- `src/runner/script_command/feature_dispatch/deploy.rs`
- `src/runner/script_command/feature_dispatch/docs.rs`
- `src/runner/script_command/feature_dispatch/inspect.rs`
- `src/runner/script_command/feature_dispatch/mod.rs`
- `src/runner/script_command/feature_dispatch/state.rs`
- `src/runner/script_command/feature_dispatch/surface.rs`
- `src/runner/script_command/nested_error.rs`

#### src/runner/task_engine (1)

- `src/runner/task_engine/tail.rs`

#### src/runner/workspace_command (9)

- `src/runner/workspace_command/provisioning/artifact_acquisition.rs`
- `src/runner/workspace_command/provisioning/artifact_plan.rs`
- `src/runner/workspace_command/provisioning/artifact_receipt.rs`
- `src/runner/workspace_command/provisioning/artifact_release.rs`
- `src/runner/workspace_command/provisioning/deadline.rs`
- `src/runner/workspace_command/provisioning/guest_io.rs`
- `src/runner/workspace_command/provisioning/permission_prep.rs`
- `src/runner/workspace_command/provisioning/publication.rs`
- `src/runner/workspace_command/provisioning/sources.rs`

### Changed rows (144): re-verify the rating against the new text

#### crates/effigy-builtin/src (6)

- `crates/effigy-builtin/src/completion/install.rs`
- `crates/effigy-builtin/src/completion/prompt.rs`
- `crates/effigy-builtin/src/config/output.rs`
- `crates/effigy-builtin/src/config/reference.rs`
- `crates/effigy-builtin/src/ports.rs`
- `crates/effigy-builtin/src/test/execution.rs`

#### crates/effigy-cli/src (4)

- `crates/effigy-cli/src/command_parsing_bootstrap.rs`
- `crates/effigy-cli/src/command_parsing_container.rs`
- `crates/effigy-cli/src/header.rs`
- `crates/effigy-cli/src/usage_hint.rs`

#### crates/effigy-containers/src (31)

- `crates/effigy-containers/src/colima.rs`
- `crates/effigy-containers/src/environment/reconcile.rs`
- `crates/effigy-containers/src/environment/remote/recovery_binding.rs`
- `crates/effigy-containers/src/environment/workspace_activation.rs`
- `crates/effigy-containers/src/exec.rs`
- `crates/effigy-containers/src/exec/colima_cleanup.rs`
- `crates/effigy-containers/src/exec/colima_runtime.rs`
- `crates/effigy-containers/src/exec/healthcheck_pidfiles.rs`
- `crates/effigy-containers/src/exec/implementation.rs`
- `crates/effigy-containers/src/exec/parse.rs`
- `crates/effigy-containers/src/exec/participation.rs`
- `crates/effigy-containers/src/exec/process.rs`
- `crates/effigy-containers/src/exec/ssh_agent_preflight.rs`
- `crates/effigy-containers/src/lib.rs`
- `crates/effigy-containers/src/manager.rs`
- `crates/effigy-containers/src/mount_spec.rs`
- `crates/effigy-containers/src/policy/data_from.rs`
- `crates/effigy-containers/src/policy/host_alias.rs`
- `crates/effigy-containers/src/policy/host_process_env.rs`
- `crates/effigy-containers/src/policy/hosts.rs`
- `crates/effigy-containers/src/policy/inline_workspace.rs`
- `crates/effigy-containers/src/policy/ports.rs`
- `crates/effigy-containers/src/policy/render/healthcheck.rs`
- `crates/effigy-containers/src/policy/render/vm.rs`
- `crates/effigy-containers/src/policy/restart.rs`
- `crates/effigy-containers/src/policy_support/generated_compose.rs`
- `crates/effigy-containers/src/report.rs`
- `crates/effigy-containers/src/runtime/dns.rs`
- `crates/effigy-containers/src/session.rs`
- `crates/effigy-containers/src/workspace/compose_rewrite.rs`
- `crates/effigy-containers/src/workspace/ownership.rs`

#### crates/effigy-exec/src (2)

- `crates/effigy-exec/src/alias.rs`
- `crates/effigy-exec/src/cwd.rs`

#### crates/effigy-kernel/src (5)

- `crates/effigy-kernel/src/fs.rs`
- `crates/effigy-kernel/src/gate.rs`
- `crates/effigy-kernel/src/root.rs`
- `crates/effigy-kernel/src/run/mod.rs`
- `crates/effigy-kernel/src/store.rs`

#### crates/effigy-manifest/src (9)

- `crates/effigy-manifest/src/bundles.rs`
- `crates/effigy-manifest/src/composition.rs`
- `crates/effigy-manifest/src/config_sections.rs`
- `crates/effigy-manifest/src/config_sections/secrets.rs`
- `crates/effigy-manifest/src/ingress.rs`
- `crates/effigy-manifest/src/lib.rs`
- `crates/effigy-manifest/src/retired_notice.rs`
- `crates/effigy-manifest/src/retired_sections.rs`
- `crates/effigy-manifest/src/user_config.rs`

#### crates/effigy-process/src (1)

- `crates/effigy-process/src/lib.rs`

#### crates/effigy-tasks/src (6)

- `crates/effigy-tasks/src/listing.rs`
- `crates/effigy-tasks/src/qa_groups/mod.rs`
- `crates/effigy-tasks/src/qa_groups/plan.rs`
- `crates/effigy-tasks/src/qa_groups/render.rs`
- `crates/effigy-tasks/src/testing/vitest.rs`
- `crates/effigy-tasks/src/view.rs`

#### crates/effigy-tui/src (2)

- `crates/effigy-tui/src/multiprocess/lifecycle`
- `crates/effigy-tui/src/multiprocess/mod.rs`

#### crates/effigy-ui/src (3)

- `crates/effigy-ui/src/output.rs`
- `crates/effigy-ui/src/plain_renderer`
- `crates/effigy-ui/src/steps.rs`

#### effigy-catalog (1)

- `effigy-catalog::fragment`

#### effigy-core (4)

- `effigy-core::build_info`
- `effigy-core::effigy_invocation`
- `effigy-core::resolver`
- `effigy-core::worktree_scope`

#### effigy-deps (2)

- `effigy-deps::bun`
- `effigy-deps::status`

#### effigy-doctor (2)

- `effigy-doctor::deep_inventory`
- `effigy-doctor::health`

#### effigy-release (1)

- `effigy-release::run`

#### effigy-runtime (4)

- `effigy-runtime::data`
- `effigy-runtime::qa_group_status`
- `effigy-runtime::read`
- `effigy-runtime::shell`

#### effigy-scan (3)

- `effigy-scan::error`
- `effigy-scan::execution`
- `effigy-scan::support`

#### effigy-state (1)

- `effigy-state::history`

#### gateway (5)

- `gateway::endpoint_store`
- `gateway::error`
- `gateway::lib`
- `gateway::ports`
- `gateway::tls`

#### host_process (3)

- `host_process`
- `host_process::kernel_run`
- `host_process::waits`

#### managed (5)

- `managed::lib`
- `managed::plan`
- `managed::references`
- `managed::render_support`
- `managed::run_spec`

#### managed_run (3)

- `managed_run::engine`
- `managed_run::environment`
- `managed_run::tui`

#### runner (11)

- `runner::builtin_ports`
- `runner::doctor_ports`
- `runner::error`
- `runner::gateway_command`
- `runner::mod`
- `runner::program_fallback`
- `runner::scratch`
- `runner::secret_vault`
- `runner::secrets_command`
- `runner::state_command`
- `runner::task_engine`

#### src/runner/container_command (13)

- `src/runner/container_command/data.rs`
- `src/runner/container_command/gateway_registration/mod.rs`
- `src/runner/container_command/handover.rs`
- `src/runner/container_command/lifecycle.rs`
- `src/runner/container_command/mod.rs`
- `src/runner/container_command/profile.rs`
- `src/runner/container_command/readiness.rs`
- `src/runner/container_command/scope.rs`
- `src/runner/container_command/secret_env.rs`
- `src/runner/container_command/shared_data.rs`
- `src/runner/container_command/shell_prep.rs`
- `src/runner/container_command/support.rs`
- `src/runner/container_command/workspace_artifact_builder.rs`

#### src/runner/container_runtime_prep (2)

- `src/runner/container_runtime_prep/mod.rs`
- `src/runner/container_runtime_prep/prep.rs`

#### src/runner/exec_command (3)

- `src/runner/exec_command/bounded.rs`
- `src/runner/exec_command/transport.rs`
- `src/runner/exec_command/transport/colima.rs`

#### src/runner/execute (7)

- `src/runner/execute/binding.rs`
- `src/runner/execute/host_alias.rs`
- `src/runner/execute/pipeline`
- `src/runner/execute/pipeline/managed.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/execute/preflight`
- `src/runner/execute/sequence_run.rs`

#### src/runner/workspace_command (5)

- `src/runner/workspace_command/mod.rs`
- `src/runner/workspace_command/permissions.rs`
- `src/runner/workspace_command/provisioning.rs`
- `src/runner/workspace_command/reserved_guest.rs`
- `src/runner/workspace_command/staging.rs`
