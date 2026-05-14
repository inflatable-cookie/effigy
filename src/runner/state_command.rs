use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_artifacts::OciArtifactAdapter;
use effigy_cli::{BootstrapDbSeedInput, StateArgs, StateSubcommand, TaskInvocation};
use effigy_execution::ExecutionSurface;
use effigy_manifest::ManifestTaskOrReferenceDefinition;
use effigy_state::{
    build_state_apply_hook_context, build_state_capture_task_context, capture_produced_layer,
    capture_profile_from_state_value, load_state_stack_manifest_file, mark_skipped_apply_layers,
    parse_capture_role, parse_state_history_kind, path_display, plain_state_environment,
    plain_state_layer_apply_mode, plain_state_layer_role, resolve_capture_request,
    select_state_stack_for_apply, select_state_stack_manifest, state_apply_hook_environment,
    state_capture_set_report_write_paths, state_capture_task_environment, state_report_write_paths,
    state_task_definition_into_manifest_task, write_state_context_file, write_state_report,
    ResolvedStateStackForApply, StateCaptureArtifactOperation, StateCaptureMode,
    StateCapturePlanRequest, StateCaptureRequestDefinition, StateCaptureSetEntry,
    StateCaptureSetReport, StateHistoryKind, StateLayerRole, StateStackApplyHookStatus,
    StateStackApplyLayerReport, StateStackApplyLayerStatus, StateStackApplyReport,
    StateStackCaptureArtifact, StateStackCaptureReport, StateStackCaptureTask,
    StateStackCaptureTaskStatus, StateStackHistoryReport, StateStackLineageReport,
    StateStackManifest,
};
use serde_json::Value;

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;
use crate::runner::manifest::load_task_manifest_with_inspection;
use crate::runner::state_command_render::{
    render_state_apply_text, render_state_capture_set_text, render_state_capture_text,
    render_state_history_text, render_state_plan_text,
};

pub(super) fn run_state(args: StateArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    match args.subcommand {
        StateSubcommand::Plan {
            manifest,
            stack,
            write_report,
        } => run_state_plan(
            manifest.as_deref(),
            stack.as_deref(),
            &context.invocation_cwd,
            &context.resolved.resolved_root,
            write_report,
            args.output_json,
        ),
        StateSubcommand::Apply {
            manifest,
            stack,
            yes,
            skip_layers,
        } => run_state_apply(
            manifest.as_deref(),
            stack.as_deref(),
            &context.invocation_cwd,
            &context.resolved.resolved_root,
            yes,
            &skip_layers,
            args.output_json,
        ),
        StateSubcommand::Capture {
            manifest,
            stack,
            profile,
            role,
            source_env,
            key,
            source,
            destination_ref,
            hook,
            task,
            yes,
            push,
        } => run_state_capture(
            manifest.as_deref(),
            stack.as_deref(),
            &context.invocation_cwd,
            &context.resolved.resolved_root,
            StateCaptureRequest {
                role,
                source_env,
                key,
                profile,
                source,
                destination_ref,
                hook,
                task: task.map(ManifestTaskOrReferenceDefinition::Reference),
                yes,
                push,
            },
            args.output_json,
        ),
        StateSubcommand::CaptureSet {
            stack,
            profiles,
            key,
            yes,
            push,
        } => run_state_capture_set(
            &stack,
            &profiles,
            key,
            &context.invocation_cwd,
            &context.resolved.resolved_root,
            yes,
            push,
            args.output_json,
        ),
        StateSubcommand::History {
            stack,
            kind,
            limit,
            lineage,
        } => run_state_history(
            &context.resolved.resolved_root,
            &stack,
            kind.as_deref(),
            limit,
            lineage.as_deref(),
            args.output_json,
        ),
    }
}

fn run_state_plan(
    manifest: Option<&Path>,
    stack: Option<&str>,
    invocation_cwd: &Path,
    repo_root: &Path,
    write_report: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let manifest = resolve_state_stack_manifest(manifest, stack, invocation_cwd, repo_root)?;
    let mut report = manifest
        .plan_lineage()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .report("planned");
    if write_report {
        let paths = state_report_write_paths(
            repo_root,
            &report.stack_name,
            StateHistoryKind::Plan,
            Some(&report.lineage_id),
            Some("plan.json"),
        );
        report.written_report_path = paths
            .compatibility_path
            .as_ref()
            .map(|path| path_display(path, repo_root));
        report.written_history_path = Some(path_display(&paths.history_path, repo_root));
        write_state_report(repo_root, &paths, &report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_plan_text(&report))
}

fn run_state_apply(
    manifest: Option<&Path>,
    stack: Option<&str>,
    invocation_cwd: &Path,
    repo_root: &Path,
    execute: bool,
    skip_layers: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let resolved = resolve_state_stack_for_apply(manifest, stack, invocation_cwd, repo_root)?;
    let lineage = resolved
        .manifest
        .plan_lineage()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .report("planned");
    let mut report = StateStackApplyReport::from_lineage(&lineage, execute);
    mark_skipped_apply_layers(&mut report, skip_layers)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if execute {
        validate_sql_layers(repo_root, &report.layers)?;
        for layer in &mut report.layers {
            match layer.status {
                StateStackApplyLayerStatus::PlannedTask => {
                    match crate::runner::execute::api::run_manifest_task_with_surface(
                        &TaskInvocation {
                            name: layer.source.clone(),
                            args: Vec::new(),
                        },
                        repo_root.to_path_buf(),
                        ExecutionSurface::DirectCli,
                    ) {
                        Ok(output) => {
                            layer.status = StateStackApplyLayerStatus::Executed;
                            layer.output = Some(output);
                            if let Err(error) = execute_state_apply_hook(
                                &report.stack_name,
                                report.environment,
                                &report.lineage_id,
                                layer,
                                repo_root,
                                resolved.hooks.get(&layer.key),
                            ) {
                                layer.hook_status = Some(StateStackApplyHookStatus::Failed);
                                layer.hook_error = Some(error.to_string());
                                report.ok = false;
                                break;
                            }
                        }
                        Err(error) => {
                            layer.status = StateStackApplyLayerStatus::Failed;
                            layer.error = Some(error.to_string());
                            report.ok = false;
                            break;
                        }
                    }
                }
                StateStackApplyLayerStatus::PlannedArtifactStage => {
                    match crate::runner::artifact_command::stage_artifact_report(
                        &layer.source,
                        repo_root,
                        repo_root,
                        false,
                        &crate::runner::artifact_transport::OrasCliArtifactAdapter::default(),
                    ) {
                        Ok(artifact_report) => {
                            layer.status = StateStackApplyLayerStatus::Staged;
                            layer.artifact_report = Some(artifact_report);
                            if let Err(error) = execute_state_apply_hook(
                                &report.stack_name,
                                report.environment,
                                &report.lineage_id,
                                layer,
                                repo_root,
                                resolved.hooks.get(&layer.key),
                            ) {
                                layer.hook_status = Some(StateStackApplyHookStatus::Failed);
                                layer.hook_error = Some(error.to_string());
                                report.ok = false;
                                break;
                            }
                        }
                        Err(error) => {
                            layer.status = StateStackApplyLayerStatus::Failed;
                            layer.error = Some(error.to_string());
                            report.ok = false;
                            break;
                        }
                    }
                }
                StateStackApplyLayerStatus::PlannedSqlImport => {
                    match crate::runner::db_seed::run_db_seed_import_report(
                        repo_root,
                        PathBuf::from(&layer.source),
                        layer.target.clone(),
                    ) {
                        Ok(sql_report) => {
                            layer.status = StateStackApplyLayerStatus::Imported;
                            layer.sql_report = Some(sql_report);
                            if let Err(error) = execute_state_apply_hook(
                                &report.stack_name,
                                report.environment,
                                &report.lineage_id,
                                layer,
                                repo_root,
                                resolved.hooks.get(&layer.key),
                            ) {
                                layer.hook_status = Some(StateStackApplyHookStatus::Failed);
                                layer.hook_error = Some(error.to_string());
                                report.ok = false;
                                break;
                            }
                        }
                        Err(error) => {
                            layer.status = StateStackApplyLayerStatus::Failed;
                            layer.error = Some(error.to_string());
                            report.ok = false;
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let paths = state_report_write_paths(
        repo_root,
        &report.stack_name,
        StateHistoryKind::Apply,
        Some(&report.lineage_id),
        None,
    );
    report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
    report.written_history_path = Some(path_display(&paths.history_path, repo_root));
    write_state_report(repo_root, &paths, &report)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if execute && !report.ok {
        let report_path = report
            .written_report_path
            .as_deref()
            .unwrap_or("<not written>");
        return Err(RunnerError::task_invocation(format!(
            "state apply failed; report: {report_path}"
        )));
    }

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_apply_text(&report))
}

fn execute_state_apply_hook(
    stack_name: &str,
    environment: effigy_state::StateEnvironment,
    lineage_id: &str,
    layer: &mut StateStackApplyLayerReport,
    repo_root: &Path,
    hook_definition: Option<&ManifestTaskOrReferenceDefinition>,
) -> Result<(), RunnerError> {
    let Some(hook) = layer.hook.clone() else {
        return Ok(());
    };
    let context_path =
        write_state_apply_hook_context(repo_root, stack_name, environment, lineage_id, layer)?;
    layer.hook_context_path = Some(context_path.clone());
    let hook_env = state_apply_hook_env(stack_name, environment, lineage_id, layer, &context_path);
    let result = match hook_definition {
        Some(ManifestTaskOrReferenceDefinition::Reference(name)) => {
            crate::runner::execute::api::run_manifest_task_with_surface_env_and_secret_targets(
                &TaskInvocation {
                    name: name.clone(),
                    args: Vec::new(),
                },
                repo_root.to_path_buf(),
                ExecutionSurface::DirectCli,
                &hook_env,
                &["state"],
            )
        }
        None => crate::runner::execute::api::run_manifest_task_with_surface_env_and_secret_targets(
            &TaskInvocation {
                name: hook,
                args: Vec::new(),
            },
            repo_root.to_path_buf(),
            ExecutionSurface::DirectCli,
            &hook_env,
            &["state"],
        ),
        Some(definition) => state_task_definition_into_manifest_task(definition.clone())
            .ok_or_else(|| RunnerError::task_invocation("missing inline apply hook".to_owned()))
            .and_then(|inline_task| {
                crate::runner::execute::api::run_inline_task_with_cwd_and_env(
                    inline_task,
                    repo_root.to_path_buf(),
                    "state apply inline hook",
                    &hook_env,
                )
            }),
    };
    match result {
        Ok(output) => {
            layer.hook_status = Some(StateStackApplyHookStatus::Executed);
            layer.hook_output = Some(output);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn validate_sql_layers(
    repo_root: &Path,
    layers: &[StateStackApplyLayerReport],
) -> Result<(), RunnerError> {
    let db_seeds = layers
        .iter()
        .filter(|layer| layer.status == StateStackApplyLayerStatus::PlannedSqlImport)
        .map(|layer| BootstrapDbSeedInput {
            target: layer.target.clone(),
            path: PathBuf::from(&layer.source),
        })
        .collect::<Vec<_>>();
    if db_seeds.is_empty() {
        return Ok(());
    }
    crate::runner::db_seed::validate_db_seed_import_inputs(repo_root, &db_seeds)
}

fn run_state_capture(
    manifest: Option<&Path>,
    stack: Option<&str>,
    invocation_cwd: &Path,
    repo_root: &Path,
    request: StateCaptureRequest,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = run_state_capture_report(manifest, stack, invocation_cwd, repo_root, request)?;

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_capture_text(&report))
}

fn run_state_capture_report(
    manifest: Option<&Path>,
    stack: Option<&str>,
    invocation_cwd: &Path,
    repo_root: &Path,
    request: StateCaptureRequest,
) -> Result<StateStackCaptureReport, RunnerError> {
    let request = resolve_state_capture_request(repo_root, stack, manifest, request)?;
    let manifest = resolve_state_stack_manifest(manifest, stack, invocation_cwd, repo_root)?;
    let lineage = manifest
        .plan_lineage()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .report("planned");
    let adapter = crate::runner::artifact_transport::OrasCliArtifactAdapter::default();
    let mut report = build_state_stack_capture_report(&lineage, request, repo_root, &adapter)?;
    let paths = state_report_write_paths(
        repo_root,
        &report.stack_name,
        StateHistoryKind::Capture,
        Some(&report.parent_lineage_id),
        None,
    );
    report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
    report.written_history_path = Some(path_display(&paths.history_path, repo_root));
    write_state_report(repo_root, &paths, &report)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    Ok(report)
}

fn run_state_capture_set(
    stack: &str,
    profiles: &[String],
    key: Option<String>,
    invocation_cwd: &Path,
    repo_root: &Path,
    yes: bool,
    push: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let key = key.unwrap_or_else(default_capture_set_key);
    let mut captures = Vec::new();
    let mut ok = true;

    for profile in profiles {
        match run_state_capture_report(
            None,
            Some(stack),
            invocation_cwd,
            repo_root,
            StateCaptureRequest {
                profile: Some(profile.clone()),
                key: Some(key.clone()),
                role: None,
                source_env: None,
                source: None,
                destination_ref: None,
                hook: None,
                task: None,
                yes,
                push,
            },
        ) {
            Ok(report) => {
                ok &= report.ok;
                captures.push(StateCaptureSetEntry {
                    profile: profile.clone(),
                    ok: report.ok,
                    report: Some(report),
                    error: None,
                });
            }
            Err(error) => {
                ok = false;
                captures.push(StateCaptureSetEntry {
                    profile: profile.clone(),
                    ok: false,
                    report: None,
                    error: Some(error.to_string()),
                });
                break;
            }
        }
    }

    let report = StateCaptureSetReport {
        schema: "effigy.state-stack.capture-set.v1".to_owned(),
        schema_version: 1,
        ok,
        executed: yes,
        stack: stack.to_owned(),
        key: key.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        profiles: profiles.to_vec(),
        captures,
        written_report_path: None,
        written_history_path: None,
    };
    let mut report = report;
    let paths = state_capture_set_report_write_paths(repo_root, stack, &key);
    report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
    report.written_history_path = Some(path_display(&paths.history_path, repo_root));
    write_state_report(repo_root, &paths, &report)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_capture_set_text(&report))
}

fn run_state_history(
    repo_root: &Path,
    stack: &str,
    kind: Option<&str>,
    limit: Option<usize>,
    lineage: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let kind = kind
        .map(parse_state_history_kind)
        .transpose()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let resolved_stack = resolve_state_history_stack_name(repo_root, stack);
    let report = StateStackHistoryReport::scan(
        repo_root,
        &resolved_stack,
        kind,
        limit.unwrap_or(20),
        lineage,
    );

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_history_text(&report))
}

fn resolve_state_history_stack_name(repo_root: &Path, stack: &str) -> String {
    load_composed_manifest_stack(repo_root, Some(stack))
        .map(|manifest| manifest.name)
        .unwrap_or_else(|_| stack.to_owned())
}

fn resolve_state_stack_manifest(
    manifest: Option<&Path>,
    stack: Option<&str>,
    invocation_cwd: &Path,
    repo_root: &Path,
) -> Result<StateStackManifest, RunnerError> {
    if let Some(manifest) = manifest {
        if stack.is_some() {
            return Err(RunnerError::task_invocation(
                "`--stack` selects from `[state.stacks]` and cannot be combined with a standalone state-stack manifest".to_owned(),
            ));
        }
        return load_state_stack_manifest_file(manifest, invocation_cwd, repo_root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    load_composed_manifest_stack(repo_root, stack)
}

fn load_composed_manifest_stack(
    repo_root: &Path,
    stack: Option<&str>,
) -> Result<StateStackManifest, RunnerError> {
    load_composed_state_value(repo_root).and_then(|state_value| {
        select_state_stack_manifest(state_value, stack)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))
    })
}

fn resolve_state_stack_for_apply(
    manifest: Option<&Path>,
    stack: Option<&str>,
    invocation_cwd: &Path,
    repo_root: &Path,
) -> Result<ResolvedStateStackForApply, RunnerError> {
    if manifest.is_some() {
        return Ok(ResolvedStateStackForApply {
            manifest: resolve_state_stack_manifest(manifest, stack, invocation_cwd, repo_root)?,
            hooks: BTreeMap::new(),
        });
    }
    load_composed_state_value(repo_root).and_then(|state_value| {
        select_state_stack_for_apply(state_value, stack)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))
    })
}

fn resolve_state_capture_request(
    repo_root: &Path,
    stack: Option<&str>,
    manifest: Option<&Path>,
    request: StateCaptureRequest,
) -> Result<StateCaptureRequest, RunnerError> {
    let state_value = if request.profile.is_some() {
        Some(load_composed_state_value(repo_root)?)
    } else {
        None
    };
    let request = resolve_capture_request(
        stack,
        manifest,
        StateCaptureRequestDefinition::from(request),
        |stack_name, profile_name| {
            capture_profile_from_state_value(
                state_value
                    .clone()
                    .expect("profile lookup only occurs when a profile is requested"),
                stack_name,
                profile_name,
            )
        },
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(request.into())
}

#[derive(Debug)]
struct StateCaptureRequest {
    profile: Option<String>,
    role: Option<String>,
    source_env: Option<String>,
    key: Option<String>,
    source: Option<String>,
    destination_ref: Option<String>,
    hook: Option<String>,
    task: Option<ManifestTaskOrReferenceDefinition>,
    yes: bool,
    push: bool,
}

impl From<StateCaptureRequest> for StateCaptureRequestDefinition {
    fn from(value: StateCaptureRequest) -> Self {
        Self {
            profile: value.profile,
            role: value.role,
            source_env: value.source_env,
            key: value.key,
            source: value.source,
            destination_ref: value.destination_ref,
            hook: value.hook,
            task: value.task,
            yes: value.yes,
            push: value.push,
        }
    }
}

impl From<StateCaptureRequestDefinition> for StateCaptureRequest {
    fn from(value: StateCaptureRequestDefinition) -> Self {
        Self {
            profile: value.profile,
            role: value.role,
            source_env: value.source_env,
            key: value.key,
            source: value.source,
            destination_ref: value.destination_ref,
            hook: value.hook,
            task: value.task,
            yes: value.yes,
            push: value.push,
        }
    }
}

fn load_composed_state_value(repo_root: &Path) -> Result<toml::Value, RunnerError> {
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    loaded.effective_value.get("state").cloned().ok_or_else(|| {
        RunnerError::task_invocation(
            "no `[state]` section found in the composed manifest; add `[state.stacks.<name>]` or pass `effigy state plan <MANIFEST>` for a standalone state-stack manifest".to_owned(),
        )
    })
}

fn default_capture_set_key() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn build_state_stack_capture_report(
    lineage: &StateStackLineageReport,
    request: StateCaptureRequest,
    repo_root: &Path,
    adapter: &dyn OciArtifactAdapter,
) -> Result<StateStackCaptureReport, RunnerError> {
    let role = request
        .role
        .as_deref()
        .ok_or_else(|| RunnerError::task_invocation("missing state capture role".to_owned()))?;
    let source_env = request.source_env.clone().ok_or_else(|| {
        RunnerError::task_invocation("missing state capture source environment".to_owned())
    })?;
    let key = request
        .key
        .clone()
        .ok_or_else(|| RunnerError::task_invocation("missing state capture key".to_owned()))?;
    let capture_role = parse_capture_role(role).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`state capture` role must be `uat-capture` or `full-capture`, got `{role}`"
        ))
    })?;
    let capture_mode = StateCaptureMode::from_role(capture_role).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`state capture` role must be `uat-capture` or `full-capture`, got `{role}`"
        ))
    })?;
    if request.yes && request.source.is_none() {
        return Err(RunnerError::task_invocation(
                "`state capture --yes` requires `--source <PATH>` for an already-produced capture payload".to_owned(),
            ));
    }
    if request.yes && request.destination_ref.is_none() {
        return Err(RunnerError::task_invocation(
                "`state capture --yes` requires `--ref oci://<REF>` so the staged capture has an explicit future destination".to_owned(),
            ));
    }
    if request.push && !request.yes {
        return Err(RunnerError::task_invocation(
            "`state capture --push` requires `--yes` so publish is explicit".to_owned(),
        ));
    }
    let produced_layer = capture_produced_layer(
        lineage,
        capture_role,
        &StateCapturePlanRequest::new(source_env.clone(), key.clone())
            .source(request.source.clone())
            .destination_ref(request.destination_ref.clone())
            .hook(request.hook.clone()),
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut warnings = lineage.warnings.clone();
    if request.destination_ref.is_none() {
        warnings.push(
            "capture destination ref is not specified; produced layer source is unresolved"
                .to_owned(),
        );
    }
    let capture_task = request.task.clone();
    let mut tasks = capture_task
        .as_ref()
        .map(|definition| StateStackCaptureTask {
            name: definition.report_name(),
            status: StateStackCaptureTaskStatus::Planned,
            context_path: None,
            output: None,
            error: None,
        })
        .into_iter()
        .collect::<Vec<_>>();
    if request.yes {
        let context_path = if tasks.is_empty() {
            None
        } else {
            Some(write_state_capture_task_context(
                repo_root,
                lineage,
                &request,
                capture_role,
                capture_mode,
            )?)
        };
        for task in &mut tasks {
            task.context_path.clone_from(&context_path);
            let task_env = state_capture_task_env(
                repo_root,
                lineage,
                &request,
                capture_role,
                capture_mode,
                context_path.as_deref(),
            );
            let result = match capture_task.clone() {
                Some(ManifestTaskOrReferenceDefinition::Reference(name)) => {
                    crate::runner::execute::api::run_manifest_task_with_surface_and_env(
                        &TaskInvocation {
                            name,
                            args: Vec::new(),
                        },
                        repo_root.to_path_buf(),
                        ExecutionSurface::DirectCli,
                        &task_env,
                    )
                }
                Some(definition) => {
                    let inline_task = state_task_definition_into_manifest_task(definition)
                        .ok_or_else(|| {
                            RunnerError::task_invocation("missing inline capture task".to_owned())
                        });
                    inline_task.and_then(|inline_task| {
                        crate::runner::execute::api::run_inline_task_with_cwd_and_env(
                            inline_task,
                            repo_root.to_path_buf(),
                            "state capture inline task",
                            &task_env,
                        )
                    })
                }
                None => Ok(String::new()),
            };
            match result {
                Ok(output) => {
                    task.status = StateStackCaptureTaskStatus::Executed;
                    task.output = Some(output);
                }
                Err(error) => {
                    task.status = StateStackCaptureTaskStatus::Failed;
                    task.error = Some(error.to_string());
                    return Ok(StateStackCaptureReport {
                        schema: "effigy.state-stack.capture.v1".to_owned(),
                        schema_version: 1,
                        ok: false,
                        executed: true,
                        stack_name: lineage.stack_name.clone(),
                        source_environment: source_env.clone(),
                        capture_role,
                        capture_mode,
                        parent_lineage_id: lineage.lineage_id.clone(),
                        created_at: "planned".to_owned(),
                        produced_layers: vec![produced_layer],
                        capture_artifacts: vec![StateStackCaptureArtifact {
                            layer_key: key,
                            operation: StateCaptureArtifactOperation::PlannedCapture,
                            ref_: request.destination_ref,
                            digest: None,
                            artifact_report: None,
                        }],
                        tasks,
                        warnings,
                        written_report_path: None,
                        written_history_path: None,
                    });
                }
            }
        }
    }
    let artifact_report = match (
        request.yes,
        request.source.as_deref(),
        request.destination_ref.as_deref(),
    ) {
        (true, Some(source), Some(destination)) => Some(
            crate::runner::artifact_command::capture_artifact_report_with_adapter(
                source,
                destination,
                Some("app-specific"),
                Some(&source_env),
                repo_root,
                repo_root,
                false,
                request.push,
                adapter,
            )?,
        ),
        _ => None,
    };
    let digest = artifact_report
        .as_ref()
        .and_then(|report| report.pointer("/destination/digest"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let operation = if request.push && artifact_report.is_some() {
        StateCaptureArtifactOperation::Pushed
    } else if artifact_report.is_some() {
        StateCaptureArtifactOperation::CapturedLocal
    } else {
        StateCaptureArtifactOperation::PlannedCapture
    };

    Ok(StateStackCaptureReport {
        schema: "effigy.state-stack.capture.v1".to_owned(),
        schema_version: 1,
        ok: true,
        executed: request.yes,
        stack_name: lineage.stack_name.clone(),
        source_environment: source_env,
        capture_role,
        capture_mode,
        parent_lineage_id: lineage.lineage_id.clone(),
        created_at: "planned".to_owned(),
        produced_layers: vec![produced_layer],
        capture_artifacts: vec![StateStackCaptureArtifact {
            layer_key: key,
            operation,
            ref_: request.destination_ref,
            digest,
            artifact_report,
        }],
        tasks,
        warnings,
        written_report_path: None,
        written_history_path: None,
    })
}

fn state_capture_task_env(
    repo_root: &Path,
    lineage: &StateStackLineageReport,
    request: &StateCaptureRequest,
    capture_role: StateLayerRole,
    capture_mode: StateCaptureMode,
    context_path: Option<&str>,
) -> BTreeMap<String, String> {
    state_capture_task_environment(
        repo_root,
        lineage,
        request.source_env.as_deref().unwrap_or_default(),
        request.key.as_deref().unwrap_or_default(),
        request.source.as_deref(),
        request.destination_ref.as_deref(),
        capture_role,
        capture_mode,
        context_path,
    )
}

fn write_state_capture_task_context(
    repo_root: &Path,
    lineage: &StateStackLineageReport,
    request: &StateCaptureRequest,
    capture_role: StateLayerRole,
    capture_mode: StateCaptureMode,
) -> Result<String, RunnerError> {
    let built = build_state_capture_task_context(
        lineage,
        &lineage.stack_name,
        request.key.as_deref().unwrap_or("capture"),
        plain_state_layer_role(capture_role),
        capture_mode,
        request.source_env.clone().unwrap_or_default(),
        request.source.clone(),
        request.destination_ref.clone(),
    );
    write_state_context_file(
        repo_root,
        &built,
        "state capture context directory",
        "state capture context",
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn state_apply_hook_env(
    stack_name: &str,
    environment: effigy_state::StateEnvironment,
    lineage_id: &str,
    layer: &StateStackApplyLayerReport,
    context_path: &str,
) -> BTreeMap<String, String> {
    state_apply_hook_environment(stack_name, environment, lineage_id, layer, context_path)
}

fn write_state_apply_hook_context(
    repo_root: &Path,
    stack_name: &str,
    environment: effigy_state::StateEnvironment,
    lineage_id: &str,
    layer: &StateStackApplyLayerReport,
) -> Result<String, RunnerError> {
    let built = build_state_apply_hook_context(
        stack_name,
        plain_state_environment(environment),
        lineage_id,
        layer,
        plain_state_layer_role(layer.role),
        plain_state_layer_apply_mode(layer.apply_mode),
    );
    write_state_context_file(
        repo_root,
        &built,
        "state apply context directory",
        "state apply context",
    )
    .map(|path| repo_root.join(path).display().to_string())
    .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use effigy_artifacts::{
        OciArtifactDescriptor, OciArtifactError, OciArtifactInspectRequest, OciArtifactPullReport,
        OciArtifactPullRequest, OciArtifactPushReport, OciArtifactPushRequest,
    };
    use effigy_manifest::ManifestManagedRun;
    use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};
    use effigy_state::{
        StateEnvironment, StateLayerApplyMode, StateLayerEnvironmentPolicy, StateLayerRole,
        StateManifestConfig, StateStackLineageLayer, StateStackLineageReport,
    };

    use super::*;

    fn temp_repo(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "effigy-state-command-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create temp repo");
        root
    }

    struct ScopedEnvVar {
        key: String,
        previous: Option<String>,
    }

    impl ScopedEnvVar {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key: key.to_owned(),
                previous,
            }
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var(&self.key, previous);
                } else {
                    std::env::remove_var(&self.key);
                }
            }
        }
    }

    fn write_state_test_vault(root: &Path, passphrase: &str, records: &[(&str, &str)]) {
        let mut payload = VaultPlaintextPayload::empty();
        for (name, value) in records {
            payload.records.insert(
                (*name).to_owned(),
                VaultSecretRecord::new(SecretValue::new(*value)),
            );
        }
        let envelope = payload
            .encrypt_with_passphrase(passphrase)
            .expect("encrypt test vault");
        let vault_path = root.join(".effigy/secrets/local.vault");
        fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
        fs::write(
            vault_path,
            envelope.to_json_pretty().expect("serialize test vault"),
        )
        .expect("write test vault");
    }

    fn state_config_with_inline_task(task_key: &str, task_value: &str) -> String {
        format!(
            r#"
[uat]
schema = "effigy.state-stack.v1"
name = "uat"
environment = "uat"

[[uat.layers]]
key = "legacy"
role = "legacy-import"
source = "oci://example.test/acme/legacy:latest"
apply_mode = "artifact"
environment_policy = "all"
{task_key} = {task_value}

[uat.captures.media]
role = "full-capture"
source_env = "legacy"
source = ".effigy/state/captures/{{key}}/media"
task = {task_value}
"#
        )
    }

    #[test]
    fn state_layer_hook_accepts_compact_inline_task_run_in() {
        let config: StateManifestConfig = toml::from_str(&state_config_with_inline_task(
            "hook",
            r#"{ rhai = "state/apply-media.rhai", run_in = "container" }"#,
        ))
        .expect("parse state config");

        let mut resolved = config
            .select_stack_for_apply(Some("uat"))
            .expect("select state stack");
        let hook = resolved.hooks.remove("legacy").expect("hook");
        let task = hook.into_manifest_task().expect("inline hook task");

        assert_eq!(
            task.run_in,
            Some(effigy_manifest::ManifestTaskRunIn::Container)
        );
        let Some(ManifestManagedRun::Sequence(steps)) = task.run else {
            panic!("expected compact inline task to become one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [effigy_manifest::ManifestManagedRunStep::Step(step)]
                if step.rhai.as_deref() == Some("state/apply-media.rhai")
        ));
    }

    #[test]
    fn state_capture_task_accepts_compact_inline_task_run_in() {
        let config: StateManifestConfig = toml::from_str(&state_config_with_inline_task(
            "hook",
            r#"{ rhai = "state/capture-media.rhai", run_in = "host" }"#,
        ))
        .expect("parse state config");
        let profile = config
            .capture_profile("uat", "media")
            .expect("media capture");
        let task = profile
            .task
            .clone()
            .expect("capture task")
            .into_manifest_task()
            .expect("inline capture task");

        assert_eq!(task.run_in, Some(effigy_manifest::ManifestTaskRunIn::Host));
        let Some(ManifestManagedRun::Sequence(steps)) = task.run else {
            panic!("expected compact inline task to become one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [effigy_manifest::ManifestManagedRunStep::Step(step)]
                if step.rhai.as_deref() == Some("state/capture-media.rhai")
        ));
    }

    fn lineage() -> StateStackLineageReport {
        StateStackLineageReport {
            schema: effigy_state::STATE_STACK_LINEAGE_SCHEMA.to_owned(),
            lineage_id: "acowtancy-uat:Uat:structure+legacy-content".to_owned(),
            stack_name: "acowtancy-uat".to_owned(),
            environment: StateEnvironment::Uat,
            created_at: "planned".to_owned(),
            layers: vec![StateStackLineageLayer {
                index: 0,
                key: "legacy-content".to_owned(),
                role: StateLayerRole::LegacyImport,
                apply_mode: StateLayerApplyMode::Artifact,
                environment_policy: StateLayerEnvironmentPolicy::All,
                source: "legacy.sql".to_owned(),
                artifact_source: Some("legacy.sql".to_owned()),
                hook: None,
                snapshot_identity: None,
                sql_target: None,
            }],
            artifact_reports: Vec::new(),
            warnings: Vec::new(),
            written_report_path: None,
            written_history_path: None,
        }
    }

    #[test]
    fn state_capture_push_embeds_pushed_artifact_report() {
        let repo = temp_repo("capture-push");
        fs::create_dir_all(repo.join("captures")).expect("create captures dir");
        fs::write(repo.join("captures/uat.json"), "{}\n").expect("write capture payload");

        let report = build_state_stack_capture_report(
            &lineage(),
            StateCaptureRequest {
                profile: None,
                role: Some("uat-capture".to_owned()),
                source_env: Some("uat".to_owned()),
                key: Some("uat-capture-2026-05-08".to_owned()),
                source: Some("captures/uat.json".to_owned()),
                destination_ref: Some(
                    "oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08".to_owned(),
                ),
                hook: None,
                task: None,
                yes: true,
                push: true,
            },
            &repo,
            &FakeOciArtifactAdapter,
        )
        .expect("capture report");

        assert!(report.executed);
        assert_eq!(
            report.capture_artifacts[0].operation,
            StateCaptureArtifactOperation::Pushed
        );
        assert_eq!(
            report.capture_artifacts[0].digest.as_deref(),
            Some("sha256:pushdigest")
        );
        let artifact_report = report.capture_artifacts[0]
            .artifact_report
            .as_ref()
            .expect("artifact report");
        assert_eq!(artifact_report["destination"]["pushed"], true);
        assert_eq!(
            artifact_report["destination"]["digest"],
            "sha256:pushdigest"
        );
    }

    #[test]
    fn state_apply_executes_declared_artifact_layer_hook_with_context() {
        let repo = temp_repo("apply-hook-success");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[tasks.apply-hook]
run = "sh -lc 'printf \"%s\" \"$EFFIGY_STATE_APPLY_CONTEXT\" > hook-path.txt'"
"#,
        )
        .expect("write effigy manifest");
        fs::write(repo.join("payload.txt"), "payload\n").expect("write payload");
        fs::write(
            repo.join("state.toml"),
            r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "legacy-media"
role = "media-library"
source = "./payload.txt"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
target = "media"
hook = "apply-hook"
"#,
        )
        .expect("write state manifest");

        let rendered = run_state_apply(
            Some(Path::new("state.toml")),
            None,
            &repo,
            &repo,
            true,
            &[],
            true,
        )
        .expect("run state apply");
        let report: Value = serde_json::from_str(&rendered).expect("parse apply report");
        let layer = &report["layers"][0];

        assert_eq!(report["ok"], true);
        assert_eq!(layer["status"], "staged");
        assert_eq!(layer["hook"], "apply-hook");
        assert_eq!(layer["hook_status"], "executed");
        let hook_context_path = fs::read_to_string(repo.join("hook-path.txt"))
            .expect("read hook path marker")
            .trim()
            .to_owned();
        assert_eq!(layer["hook_context_path"], hook_context_path);

        let context_text =
            fs::read_to_string(repo.join(&hook_context_path)).expect("read hook context file");
        let context: Value = serde_json::from_str(&context_text).expect("parse hook context");
        assert_eq!(context["schema"], "effigy.state-stack.apply-context.v1");
        assert_eq!(context["stack_name"], "acowtancy-uat");
        assert_eq!(context["environment"], "uat");
        assert_eq!(context["layer"]["key"], "legacy-media");
        assert_eq!(context["layer"]["apply_mode"], "artifact");
        assert_eq!(context["layer"]["role"], "media-library");
        assert!(
            context["layer"]["artifact_report"]["metadata"]["primary_files"][0]
                .as_str()
                .expect("primary file")
                .ends_with("/payload.txt")
        );
    }

    #[test]
    fn state_apply_hook_receives_declared_state_secret() {
        let repo = temp_repo("apply-hook-state-secret");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"
identity = "passphrase"
unlock = "passphrase"

[secrets.keys.state_token]
required = true
targets = ["state"]

[tasks.apply-hook]
run = "sh -lc 'printf \"%s\" \"$STATE_TOKEN\" > state-secret.txt'"
"#,
        )
        .expect("write effigy manifest");
        write_state_test_vault(
            &repo,
            "vault-passphrase",
            &[("state_token", "state_secret")],
        );
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");
        fs::write(repo.join("payload.txt"), "payload\n").expect("write payload");
        fs::write(
            repo.join("state.toml"),
            r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "legacy-media"
role = "media-library"
source = "./payload.txt"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
target = "media"
hook = "apply-hook"
"#,
        )
        .expect("write state manifest");

        run_state_apply(
            Some(Path::new("state.toml")),
            None,
            &repo,
            &repo,
            true,
            &[],
            true,
        )
        .expect("run state apply");

        assert_eq!(
            fs::read_to_string(repo.join("state-secret.txt")).expect("read marker"),
            "state_secret"
        );
    }

    #[test]
    fn state_apply_accepts_inline_hook_task_in_composed_manifest() {
        let repo = temp_repo("apply-inline-hook");
        fs::write(repo.join("payload.txt"), "payload\n").expect("write payload");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[state]

[state.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.uat.layers]]
key = "legacy-media"
role = "media-library"
source = "./payload.txt"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
target = "media"
hook = [{ run = "sh -lc 'printf \"%s\" \"$EFFIGY_STATE_APPLY_CONTEXT\" > inline-hook-path.txt'" }]
"#,
        )
        .expect("write effigy manifest");

        let rendered = run_state_apply(None, Some("uat"), &repo, &repo, true, &[], true)
            .expect("run state apply");
        let report: Value = serde_json::from_str(&rendered).expect("parse apply report");
        let layer = &report["layers"][0];

        assert_eq!(report["ok"], true);
        assert_eq!(layer["status"], "staged");
        assert_eq!(layer["hook"], "<inline>");
        assert_eq!(layer["hook_status"], "executed");
        let hook_context_path = fs::read_to_string(repo.join("inline-hook-path.txt"))
            .expect("read hook path marker")
            .trim()
            .to_owned();
        assert_eq!(layer["hook_context_path"], hook_context_path);
    }

    #[test]
    fn state_apply_can_skip_selected_layers() {
        let repo = temp_repo("apply-skip-layer");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[state]

[state.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "schema"
apply_mode = "task"
environment_policy = "all"

[[state.uat.layers]]
key = "overlay"
role = "dev-overlay"
source = "overlay"
apply_mode = "task"
environment_policy = "all"

[tasks.schema]
run = "sh -lc 'echo schema > schema-ran.txt'"

[tasks.overlay]
run = "sh -lc 'echo overlay > overlay-ran.txt'"
"#,
        )
        .expect("write effigy manifest");

        let rendered = run_state_apply(
            None,
            Some("uat"),
            &repo,
            &repo,
            true,
            &["structure".to_owned()],
            true,
        )
        .expect("run state apply");
        let report: Value = serde_json::from_str(&rendered).expect("parse apply report");

        assert_eq!(report["ok"], true);
        assert_eq!(report["layers"][0]["status"], "skipped");
        assert_eq!(report["layers"][1]["status"], "executed");
        assert!(!repo.join("schema-ran.txt").exists());
        assert_eq!(
            fs::read_to_string(repo.join("overlay-ran.txt")).expect("read overlay marker"),
            "overlay\n"
        );
    }

    #[test]
    fn state_apply_rejects_unknown_skip_layers() {
        let repo = temp_repo("apply-skip-layer-missing");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[state]

[state.uat]
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "schema"
apply_mode = "task"
environment_policy = "all"

[tasks.schema]
run = "sh -lc 'echo schema > schema-ran.txt'"
"#,
        )
        .expect("write effigy manifest");

        let error = run_state_apply(
            None,
            Some("uat"),
            &repo,
            &repo,
            true,
            &["missing".to_owned()],
            true,
        )
        .expect_err("unknown skipped layer should fail");

        assert!(error
            .to_string()
            .contains("state apply skip layer(s) not found: missing"));
        assert!(!repo.join("schema-ran.txt").exists());
    }

    #[test]
    fn state_apply_reports_failed_hook_after_successful_artifact_stage() {
        let repo = temp_repo("apply-hook-failure");
        fs::write(
            repo.join("effigy.toml"),
            r#"
[tasks.apply-hook]
run = "sh -lc 'exit 12'"
"#,
        )
        .expect("write effigy manifest");
        fs::write(repo.join("payload.txt"), "payload\n").expect("write payload");
        fs::write(
            repo.join("state.toml"),
            r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "legacy-media"
role = "media-library"
source = "./payload.txt"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
target = "media"
hook = "apply-hook"
"#,
        )
        .expect("write state manifest");

        let error = run_state_apply(
            Some(Path::new("state.toml")),
            None,
            &repo,
            &repo,
            true,
            &[],
            true,
        )
        .expect_err("failed hook should fail state apply");
        assert!(error.to_string().contains("state apply failed"));

        let rendered =
            fs::read_to_string(repo.join(".effigy/reports/state/acowtancy-uat/latest-apply.json"))
                .expect("read failed apply report");
        let report: Value = serde_json::from_str(&rendered).expect("parse apply report");
        let layer = &report["layers"][0];

        assert_eq!(report["ok"], false);
        assert_eq!(layer["status"], "staged");
        assert_eq!(layer["hook"], "apply-hook");
        assert_eq!(layer["hook_status"], "failed");
        assert!(layer["hook_context_path"].as_str().is_some());
        assert!(layer["hook_error"]
            .as_str()
            .expect("hook error")
            .contains("failed"));
    }

    struct FakeOciArtifactAdapter;

    impl OciArtifactAdapter for FakeOciArtifactAdapter {
        fn inspect(
            &self,
            request: &OciArtifactInspectRequest,
        ) -> Result<OciArtifactDescriptor, OciArtifactError> {
            Ok(OciArtifactDescriptor::new(&request.reference)
                .with_digest("sha256:fakedigest")
                .with_media_type("application/vnd.oci.image.manifest.v1+json")
                .with_size(123))
        }

        fn pull(
            &self,
            _request: &OciArtifactPullRequest,
        ) -> Result<OciArtifactPullReport, OciArtifactError> {
            unreachable!("state capture push test should not pull")
        }

        fn push(
            &self,
            request: &OciArtifactPushRequest,
        ) -> Result<OciArtifactPushReport, OciArtifactError> {
            assert!(request.metadata_path.is_file());
            assert_eq!(request.primary_files.len(), 1);
            let descriptor =
                OciArtifactDescriptor::new(&request.reference).with_digest("sha256:pushdigest");
            Ok(OciArtifactPushReport {
                pushed_ref: request.reference.redacted(),
                digest: descriptor.digest.clone(),
                descriptor,
            })
        }
    }
}
