use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use effigy_artifacts::OciArtifactAdapter;
use effigy_cli::{BootstrapDbSeedInput, StateArgs, StateSubcommand, TaskInvocation};
use effigy_execution::ExecutionSurface;
use effigy_manifest::{ManifestManagedRun, ManifestTask};
use effigy_state::{
    capture_produced_layer, state_report_write_paths, StateCaptureMode, StateCapturePlanRequest,
    StateHistoryKind, StateLayerApplyMode, StateLayerRole, StateReportWritePaths,
    StateStackApplyHookStatus, StateStackApplyLayerReport, StateStackApplyLayerStatus,
    StateStackApplyReport, StateStackCaptureProducedLayer, StateStackHistoryReport,
    StateStackLineageReport, StateStackManifest,
};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;
use crate::runner::manifest::load_task_manifest_with_inspection;

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
                task: task.map(ManifestStateTaskDefinition::Reference),
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
        write_state_report(repo_root, &paths, &report)?;
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
    mark_skipped_state_apply_layers(&mut report, skip_layers)?;

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
    write_state_report(repo_root, &paths, &report)?;

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

fn mark_skipped_state_apply_layers(
    report: &mut StateStackApplyReport,
    skip_layers: &[String],
) -> Result<(), RunnerError> {
    if skip_layers.is_empty() {
        return Ok(());
    }

    let known_layers = report
        .layers
        .iter()
        .map(|layer| layer.key.as_str())
        .collect::<BTreeSet<_>>();
    let mut unknown_layers = skip_layers
        .iter()
        .filter(|layer| !known_layers.contains(layer.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    unknown_layers.sort();
    unknown_layers.dedup();
    if !unknown_layers.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "state apply skip layer(s) not found: {}",
            unknown_layers.join(", ")
        )));
    }

    let skip_layers = skip_layers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for layer in &mut report.layers {
        if skip_layers.contains(layer.key.as_str()) {
            layer.status = StateStackApplyLayerStatus::Skipped;
        }
    }

    Ok(())
}

fn execute_state_apply_hook(
    stack_name: &str,
    environment: effigy_state::StateEnvironment,
    lineage_id: &str,
    layer: &mut StateStackApplyLayerReport,
    repo_root: &Path,
    hook_definition: Option<&ManifestStateTaskDefinition>,
) -> Result<(), RunnerError> {
    let Some(hook) = layer.hook.clone() else {
        return Ok(());
    };
    let context_path =
        write_state_apply_hook_context(repo_root, stack_name, environment, lineage_id, layer)?;
    layer.hook_context_path = Some(context_path.clone());
    let hook_env = state_apply_hook_env(stack_name, environment, lineage_id, layer, &context_path);
    let result = match hook_definition {
        Some(ManifestStateTaskDefinition::Reference(name)) => {
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
        Some(definition) => definition
            .clone()
            .into_manifest_task()
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
    let mut report = StateStackCaptureReport::from_request(&lineage, request, repo_root, &adapter)?;
    let paths = state_report_write_paths(
        repo_root,
        &report.stack_name,
        StateHistoryKind::Capture,
        Some(&report.parent_lineage_id),
        None,
    );
    report.written_report_path = Some(path_display(&paths.latest_path, repo_root));
    report.written_history_path = Some(path_display(&paths.history_path, repo_root));
    write_state_report(repo_root, &paths, &report)?;

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
    write_state_report(repo_root, &paths, &report)?;

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
    let kind = kind.map(parse_state_history_kind).transpose()?;
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
        return load_standalone_manifest(manifest, invocation_cwd, repo_root);
    }

    load_composed_manifest_stack(repo_root, stack)
}

fn load_standalone_manifest(
    manifest: &Path,
    invocation_cwd: &Path,
    repo_root: &Path,
) -> Result<StateStackManifest, RunnerError> {
    let manifest_path = resolve_explicit_manifest_path(manifest, invocation_cwd, repo_root);
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read state stack manifest {}: {error}",
            manifest_path.display()
        ))
    })?;
    StateStackManifest::parse_toml(&manifest_text)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn resolve_explicit_manifest_path(
    manifest: &Path,
    invocation_cwd: &Path,
    repo_root: &Path,
) -> std::path::PathBuf {
    if manifest.is_absolute() {
        return manifest.to_path_buf();
    }
    let cwd_relative = invocation_cwd.join(manifest);
    if cwd_relative.exists() {
        return cwd_relative;
    }
    repo_root.join(manifest)
}

fn load_composed_manifest_stack(
    repo_root: &Path,
    stack: Option<&str>,
) -> Result<StateStackManifest, RunnerError> {
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let state_value = loaded
        .effective_value
        .get("state")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "no `[state]` section found in the composed manifest; add `[state.stacks.<name>]` or pass `effigy state plan <MANIFEST>` for a standalone state-stack manifest".to_owned(),
            )
        })?;
    let config: ManifestStateConfig = state_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[state]` config: {error}"
        ))
    })?;
    select_manifest_state_stack(config, stack)
}

#[derive(Debug)]
struct ResolvedStateStackForApply {
    manifest: StateStackManifest,
    hooks: BTreeMap<String, ManifestStateTaskDefinition>,
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
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let state_value = loaded
        .effective_value
        .get("state")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "no `[state]` section found in the composed manifest; add `[state.stacks.<name>]` or pass `effigy state plan <MANIFEST>` for a standalone state-stack manifest".to_owned(),
            )
        })?;
    let config: ManifestStateConfig = state_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[state]` config: {error}"
        ))
    })?;
    select_manifest_state_stack_for_apply(config, stack)
}

fn select_manifest_state_stack(
    mut config: ManifestStateConfig,
    stack: Option<&str>,
) -> Result<StateStackManifest, RunnerError> {
    let mut stacks = config.stacks;
    stacks.append(&mut config.named_stacks);
    let selected = if let Some(stack) = stack {
        stack.to_owned()
    } else if let Some(default_stack) = config.default.clone().or(config.default_stack.clone()) {
        default_stack
    } else if stacks.len() == 1 {
        stacks.keys().next().cloned().expect("one stack")
    } else if stacks.is_empty() {
        return Err(RunnerError::task_invocation(
            "`[state]` does not define any named state stacks".to_owned(),
        ));
    } else {
        return Err(RunnerError::task_invocation(format!(
            "multiple state stacks are defined; set `state.default` or pass `--stack <NAME>`: {}",
            stacks.keys().cloned().collect::<Vec<_>>().join(", ")
        )));
    };

    stacks
        .remove(&selected)
        .map(ManifestStateStackConfig::into_manifest)
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "state stack `{selected}` is not defined in `[state]`; available stacks: {}",
                stacks.keys().cloned().collect::<Vec<_>>().join(", ")
            ))
        })
}

fn select_manifest_state_stack_for_apply(
    mut config: ManifestStateConfig,
    stack: Option<&str>,
) -> Result<ResolvedStateStackForApply, RunnerError> {
    let mut stacks = config.stacks;
    stacks.append(&mut config.named_stacks);
    let selected = if let Some(stack) = stack {
        stack.to_owned()
    } else if let Some(default_stack) = config.default.clone().or(config.default_stack.clone()) {
        default_stack
    } else if stacks.len() == 1 {
        stacks.keys().next().cloned().expect("one stack")
    } else if stacks.is_empty() {
        return Err(RunnerError::task_invocation(
            "`[state]` does not define any named state stacks".to_owned(),
        ));
    } else {
        return Err(RunnerError::task_invocation(format!(
            "multiple state stacks are defined; set `state.default` or pass `--stack <NAME>`: {}",
            stacks.keys().cloned().collect::<Vec<_>>().join(", ")
        )));
    };

    stacks
        .remove(&selected)
        .map(ManifestStateStackConfig::into_apply)
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "state stack `{selected}` is not defined in `[state]`; available stacks: {}",
                stacks.keys().cloned().collect::<Vec<_>>().join(", ")
            ))
        })
}

fn resolve_state_capture_request(
    repo_root: &Path,
    stack: Option<&str>,
    manifest: Option<&Path>,
    mut request: StateCaptureRequest,
) -> Result<StateCaptureRequest, RunnerError> {
    let Some(profile_name) = request.profile.clone() else {
        require_capture_fields(&request)?;
        return Ok(request);
    };
    if manifest.is_some() {
        return Err(RunnerError::task_invocation(
            "named capture profiles are loaded from composed `[state]` config and cannot be combined with `--manifest`".to_owned(),
        ));
    }
    let stack_name = stack.ok_or_else(|| {
        RunnerError::task_invocation(
            "named capture profiles require `effigy state capture <stack> <profile>`".to_owned(),
        )
    })?;
    let profile = load_composed_capture_profile(repo_root, stack_name, &profile_name)?;
    if request.role.is_none() {
        request.role = Some(profile.role);
    }
    if request.source_env.is_none() {
        request.source_env = Some(profile.source_env);
    }
    if request.key.is_none() {
        request.key = Some(profile.key.unwrap_or_else(|| profile_name.clone()));
    }
    if request.source.is_none() {
        request.source = profile.source.map(|value| {
            expand_capture_template(
                &value,
                stack_name,
                &profile_name,
                request.key.as_deref().unwrap_or(&profile_name),
            )
        });
    }
    if request.destination_ref.is_none() {
        request.destination_ref = profile.destination_ref.map(|value| {
            expand_capture_template(
                &value,
                stack_name,
                &profile_name,
                request.key.as_deref().unwrap_or(&profile_name),
            )
        });
    }
    if request.hook.is_none() {
        request.hook = profile.hook;
    }
    if request.task.is_none() {
        request.task = profile.task;
    }
    request.push |= profile.push;
    require_capture_fields(&request)?;
    Ok(request)
}

fn load_composed_capture_profile(
    repo_root: &Path,
    stack_name: &str,
    profile_name: &str,
) -> Result<ManifestStateCaptureProfile, RunnerError> {
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let state_value = loaded
        .effective_value
        .get("state")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "no `[state]` section found in the composed manifest".to_owned(),
            )
        })?;
    let mut config: ManifestStateConfig = state_value.try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[state]` config: {error}"
        ))
    })?;
    let mut stacks = config.stacks;
    stacks.append(&mut config.named_stacks);
    let stack = stacks.remove(stack_name).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "state stack `{stack_name}` is not defined in `[state]`"
        ))
    })?;
    stack.captures.get(profile_name).cloned().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "state capture profile `{profile_name}` is not defined in `[state.{stack_name}.captures]`"
        ))
    })
}

fn require_capture_fields(request: &StateCaptureRequest) -> Result<(), RunnerError> {
    if request.role.is_none() {
        return Err(RunnerError::task_invocation(
            "`state capture` requires `--role <ROLE>` or a named capture profile".to_owned(),
        ));
    }
    if request.source_env.is_none() {
        return Err(RunnerError::task_invocation(
            "`state capture` requires `--source-env <ENV>` or a named capture profile".to_owned(),
        ));
    }
    if request.key.is_none() {
        return Err(RunnerError::task_invocation(
            "`state capture` requires `--key <LAYER_KEY>` or a named capture profile".to_owned(),
        ));
    }
    Ok(())
}

fn expand_capture_template(value: &str, stack: &str, profile: &str, key: &str) -> String {
    value
        .replace("{stack}", stack)
        .replace("{profile}", profile)
        .replace("{key}", key)
}

fn render_state_plan_text(report: &StateStackLineageReport) -> String {
    let mut lines = vec![
        "State stack plan".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("environment: {:?}", report.environment),
        format!("lineage: {}", report.lineage_id),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "layers:".to_owned(),
    ];
    for layer in &report.layers {
        lines.push(format!(
            "- {}: {:?} via {:?} ({:?})",
            layer.key, layer.role, layer.apply_mode, layer.environment_policy
        ));
    }
    if !report.artifact_reports.is_empty() {
        lines.push("artifact operations:".to_owned());
        for artifact in &report.artifact_reports {
            lines.push(format!(
                "- {}: {:?} {}",
                artifact.layer_key, artifact.operation, artifact.source_ref
            ));
        }
    }
    lines.join("\n")
}

fn render_state_apply_text(report: &StateStackApplyReport) -> String {
    let mut lines = vec![
        "State stack apply".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("environment: {:?}", report.environment),
        format!("mode: {}", if report.executed { "execute" } else { "plan" }),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "layers:".to_owned(),
    ];
    for layer in &report.layers {
        lines.push(format!(
            "- {}: {:?} via {:?} ({})",
            layer.key, layer.role, layer.apply_mode, layer.status
        ));
        if let Some(hook) = layer.hook.as_deref() {
            let hook_status = layer
                .hook_status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "not-run".to_owned());
            lines.push(format!("  hook: {hook} ({hook_status})"));
        }
        if let Some(error) = layer.error.as_deref() {
            lines.push(format!("  error: {error}"));
        }
        if let Some(error) = layer.hook_error.as_deref() {
            lines.push(format!("  hook error: {error}"));
        }
    }
    lines.join("\n")
}

fn render_state_capture_text(report: &StateStackCaptureReport) -> String {
    let mut lines = vec![
        "State stack capture".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("source environment: {}", report.source_environment),
        format!("mode: {}", report.capture_mode),
        format!(
            "execution: {}",
            if report.executed {
                "staged local artifact"
            } else {
                "plan-only"
            }
        ),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "produced layers:".to_owned(),
    ];
    for layer in &report.produced_layers {
        lines.push(format!(
            "- {}: {:?} via {:?}",
            layer.key, layer.role, layer.apply_mode
        ));
    }
    if !report.capture_artifacts.is_empty() {
        lines.push("capture artifacts:".to_owned());
        for artifact in &report.capture_artifacts {
            lines.push(format!(
                "- {}: {}",
                artifact.layer_key,
                artifact
                    .ref_
                    .as_deref()
                    .unwrap_or("destination ref not specified")
            ));
        }
    }
    lines.join("\n")
}

fn render_state_capture_set_text(report: &StateCaptureSetReport) -> String {
    let mut lines = vec![
        "State capture set".to_owned(),
        format!("stack: {}", report.stack),
        format!("key: {}", report.key),
        format!("executed: {}", report.executed),
        format!("ok: {}", report.ok),
        report
            .written_report_path
            .as_ref()
            .map(|path| format!("report: {path}"))
            .unwrap_or_else(|| "report: not written".to_owned()),
        report
            .written_history_path
            .as_ref()
            .map(|path| format!("history: {path}"))
            .unwrap_or_else(|| "history: not written".to_owned()),
        "captures:".to_owned(),
    ];
    for capture in &report.captures {
        if let Some(error) = &capture.error {
            lines.push(format!("- {}: failed ({error})", capture.profile));
        } else {
            lines.push(format!("- {}: {}", capture.profile, capture.ok));
        }
    }
    lines.join("\n")
}

fn render_state_history_text(report: &StateStackHistoryReport) -> String {
    let mut lines = vec![
        "State stack history".to_owned(),
        format!("schema: {}", report.schema),
        format!("stack: {}", report.stack_name),
        format!("reports: {}", report.reports.len()),
    ];
    for item in &report.reports {
        lines.push(format!(
            "- {}: {} ({})",
            item.kind,
            item.path,
            item.lineage_id
                .as_deref()
                .or(item.parent_lineage_id.as_deref())
                .unwrap_or("lineage unknown")
        ));
    }
    if !report.warnings.is_empty() {
        lines.push("warnings:".to_owned());
        for warning in &report.warnings {
            lines.push(format!("- {warning}"));
        }
    }
    lines.join("\n")
}

fn write_state_report<T: Serialize>(
    repo_root: &Path,
    paths: &StateReportWritePaths,
    report: &T,
) -> Result<(), RunnerError> {
    let encoded = serde_json::to_string_pretty(report)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let encoded = format!("{encoded}\n");
    for path in paths.all_paths() {
        let Some(parent) = path.parent() else {
            return Err(RunnerError::task_invocation(format!(
                "failed to resolve parent directory for {}",
                path.display()
            )));
        };
        fs::create_dir_all(parent).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create state report directory {}: {error}",
                parent.display()
            ))
        })?;
        fs::write(path, &encoded).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write state report {}: {error}",
                path_display(path, repo_root)
            ))
        })?;
    }
    Ok(())
}

fn state_capture_set_report_write_paths(
    repo_root: &Path,
    stack_name: &str,
    key: &str,
) -> StateReportWritePaths {
    let stack_dir = repo_root
        .join(".effigy")
        .join("reports")
        .join("state")
        .join(safe_path_component(stack_name));
    let latest_path = stack_dir.join("latest-capture-set.json");
    let history_path = stack_dir.join("history").join(format!(
        "{}-capture-set-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        safe_path_component(key)
    ));
    StateReportWritePaths {
        compatibility_path: None,
        latest_path,
        history_path,
    }
}

fn safe_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn path_display(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[derive(Debug, Deserialize)]
struct ManifestStateConfig {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    default_stack: Option<String>,
    #[serde(default)]
    stacks: BTreeMap<String, ManifestStateStackConfig>,
    #[serde(flatten)]
    named_stacks: BTreeMap<String, ManifestStateStackConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestStateStackConfig {
    schema: String,
    name: String,
    environment: effigy_state::StateEnvironment,
    layers: Vec<ManifestStateStackLayerConfig>,
    #[serde(default)]
    captures: BTreeMap<String, ManifestStateCaptureProfile>,
    #[serde(default)]
    #[allow(dead_code)]
    targets: BTreeMap<String, toml::Value>,
}

impl ManifestStateStackConfig {
    fn into_manifest(self) -> StateStackManifest {
        let Self {
            schema,
            name,
            environment,
            layers,
            captures: _,
            targets: _,
        } = self;
        StateStackManifest {
            schema,
            name,
            environment,
            layers: layers
                .into_iter()
                .map(ManifestStateStackLayerConfig::into_layer)
                .collect(),
        }
    }

    fn into_apply(self) -> ResolvedStateStackForApply {
        let mut hooks = BTreeMap::new();
        let layers = self
            .layers
            .into_iter()
            .map(|layer| {
                let key = layer.key.clone();
                let (state_layer, hook_definition) = layer.into_layer_and_hook();
                if let Some(definition) = hook_definition {
                    hooks.insert(key, definition);
                }
                state_layer
            })
            .collect();
        ResolvedStateStackForApply {
            manifest: StateStackManifest {
                schema: self.schema,
                name: self.name,
                environment: self.environment,
                layers,
            },
            hooks,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestStateStackLayerConfig {
    key: String,
    role: StateLayerRole,
    source: String,
    apply_mode: StateLayerApplyMode,
    environment_policy: effigy_state::StateLayerEnvironmentPolicy,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    artifact_kind: Option<effigy_artifacts::ArtifactKind>,
    #[serde(default)]
    snapshot_identity: Option<String>,
    #[serde(default)]
    hook: Option<ManifestStateTaskDefinition>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default, alias = "target")]
    sql_target: Option<String>,
}

impl ManifestStateStackLayerConfig {
    fn into_layer(self) -> effigy_state::StateStackLayer {
        self.into_layer_and_hook().0
    }

    fn into_layer_and_hook(
        self,
    ) -> (
        effigy_state::StateStackLayer,
        Option<ManifestStateTaskDefinition>,
    ) {
        let hook_label = self
            .hook
            .as_ref()
            .map(ManifestStateTaskDefinition::report_name);
        (
            effigy_state::StateStackLayer {
                key: self.key,
                role: self.role,
                source: self.source,
                apply_mode: self.apply_mode,
                environment_policy: self.environment_policy,
                depends_on: self.depends_on,
                artifact_kind: self.artifact_kind,
                snapshot_identity: self.snapshot_identity,
                hook: hook_label,
                notes: self.notes,
                sql_target: self.sql_target,
            },
            self.hook,
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestStateCaptureProfile {
    role: String,
    #[serde(alias = "source_environment")]
    source_env: String,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, alias = "ref")]
    destination_ref: Option<String>,
    #[serde(default)]
    hook: Option<String>,
    #[serde(default)]
    task: Option<ManifestStateTaskDefinition>,
    #[serde(default)]
    push: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ManifestStateTaskDefinition {
    Reference(String),
    Run(ManifestManagedRun),
    Task(Box<ManifestTask>),
}

impl ManifestStateTaskDefinition {
    fn report_name(&self) -> String {
        match self {
            Self::Reference(name) => name.clone(),
            Self::Run(_) | Self::Task(_) => "<inline>".to_owned(),
        }
    }

    fn into_manifest_task(self) -> Option<ManifestTask> {
        match self {
            Self::Reference(_) => None,
            Self::Run(run) => Some(ManifestTask {
                run: Some(run),
                run_in: Some(effigy_manifest::ManifestTaskRunIn::Host),
                ..Default::default()
            }),
            Self::Task(task) => Some(*task),
        }
    }
}

fn parse_state_history_kind(kind: &str) -> Result<StateHistoryKind, RunnerError> {
    StateHistoryKind::parse(kind).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`state history --kind` must be `plan`, `apply`, or `capture`, got `{kind}`"
        ))
    })
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
    task: Option<ManifestStateTaskDefinition>,
    yes: bool,
    push: bool,
}

#[derive(Debug, Serialize)]
struct StateCaptureSetReport {
    schema: String,
    schema_version: u8,
    ok: bool,
    executed: bool,
    stack: String,
    key: String,
    created_at: String,
    profiles: Vec<String>,
    captures: Vec<StateCaptureSetEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_history_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct StateCaptureSetEntry {
    profile: String,
    ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    report: Option<StateStackCaptureReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct StateStackCaptureReport {
    schema: String,
    schema_version: u8,
    ok: bool,
    executed: bool,
    stack_name: String,
    source_environment: String,
    capture_role: StateLayerRole,
    capture_mode: StateCaptureMode,
    parent_lineage_id: String,
    created_at: String,
    produced_layers: Vec<StateStackCaptureProducedLayer>,
    capture_artifacts: Vec<StateStackCaptureArtifact>,
    tasks: Vec<StateStackCaptureTask>,
    warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_history_path: Option<String>,
}

fn default_capture_set_key() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

impl StateStackCaptureReport {
    fn from_request(
        lineage: &StateStackLineageReport,
        request: StateCaptureRequest,
        repo_root: &Path,
        adapter: &dyn OciArtifactAdapter,
    ) -> Result<Self, RunnerError> {
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
        let capture_role = parse_capture_role(role)?;
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
                    Some(ManifestStateTaskDefinition::Reference(name)) => {
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
                        let inline_task = definition.into_manifest_task().ok_or_else(|| {
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
                        return Ok(Self {
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

        Ok(Self {
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
}

fn state_capture_task_env(
    repo_root: &Path,
    lineage: &StateStackLineageReport,
    request: &StateCaptureRequest,
    capture_role: StateLayerRole,
    capture_mode: StateCaptureMode,
    context_path: Option<&str>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "EFFIGY_STATE_CAPTURE_SCHEMA".to_owned(),
        "effigy.state-stack.capture.v1".to_owned(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_STACK".to_owned(),
        lineage.stack_name.clone(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_PARENT_LINEAGE_ID".to_owned(),
        lineage.lineage_id.clone(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_ROLE".to_owned(),
        serde_plain_role(capture_role),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_MODE".to_owned(),
        capture_mode.to_string(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_SOURCE_ENV".to_owned(),
        request.source_env.clone().unwrap_or_default(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_KEY".to_owned(),
        request.key.clone().unwrap_or_default(),
    );
    if let Some(source) = request.source.as_ref() {
        env.insert(
            "EFFIGY_STATE_CAPTURE_SOURCE".to_owned(),
            resolve_repo_relative_env_path(repo_root, source),
        );
    }
    if let Some(destination) = request.destination_ref.as_ref() {
        env.insert(
            "EFFIGY_STATE_CAPTURE_DESTINATION_REF".to_owned(),
            destination.clone(),
        );
    }
    if let Some(context_path) = context_path {
        env.insert(
            "EFFIGY_STATE_CAPTURE_CONTEXT".to_owned(),
            context_path.to_owned(),
        );
    }
    env
}

fn resolve_repo_relative_env_path(repo_root: &Path, path: &str) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        path.display().to_string()
    } else {
        repo_root.join(path).display().to_string()
    }
}

fn write_state_capture_task_context(
    repo_root: &Path,
    lineage: &StateStackLineageReport,
    request: &StateCaptureRequest,
    capture_role: StateLayerRole,
    capture_mode: StateCaptureMode,
) -> Result<String, RunnerError> {
    let relative_path = PathBuf::from(".effigy")
        .join("state")
        .join("capture-context")
        .join(safe_path_component(&lineage.stack_name))
        .join(format!(
            "{}.json",
            safe_path_component(request.key.as_deref().unwrap_or("capture"))
        ));
    let absolute_path = repo_root.join(&relative_path);
    let Some(parent) = absolute_path.parent() else {
        return Err(RunnerError::task_invocation(format!(
            "failed to resolve parent directory for {}",
            absolute_path.display()
        )));
    };
    fs::create_dir_all(parent).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create state capture context directory {}: {error}",
            parent.display()
        ))
    })?;
    let context = StateCaptureTaskContext {
        schema: "effigy.state-stack.capture-context.v1".to_owned(),
        schema_version: 1,
        stack_name: lineage.stack_name.clone(),
        parent_lineage_id: lineage.lineage_id.clone(),
        capture_role: serde_plain_role(capture_role),
        capture_mode: capture_mode.to_string(),
        source_environment: request.source_env.clone().unwrap_or_default(),
        key: request.key.clone().unwrap_or_default(),
        source: request.source.clone(),
        destination_ref: request.destination_ref.clone(),
    };
    let encoded = serde_json::to_string_pretty(&context)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    fs::write(&absolute_path, format!("{encoded}\n")).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write state capture context {}: {error}",
            path_display(&absolute_path, repo_root)
        ))
    })?;
    Ok(path_display(&absolute_path, repo_root))
}

fn state_apply_hook_env(
    stack_name: &str,
    environment: effigy_state::StateEnvironment,
    lineage_id: &str,
    layer: &StateStackApplyLayerReport,
    context_path: &str,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "EFFIGY_STATE_APPLY_SCHEMA".to_owned(),
        "effigy.state-stack.apply.v1".to_owned(),
    );
    env.insert("EFFIGY_STATE_APPLY_STACK".to_owned(), stack_name.to_owned());
    env.insert(
        "EFFIGY_STATE_APPLY_ENVIRONMENT".to_owned(),
        serde_plain_state_environment(environment),
    );
    env.insert(
        "EFFIGY_STATE_APPLY_LINEAGE_ID".to_owned(),
        lineage_id.to_owned(),
    );
    env.insert("EFFIGY_STATE_APPLY_LAYER_KEY".to_owned(), layer.key.clone());
    env.insert(
        "EFFIGY_STATE_APPLY_LAYER_ROLE".to_owned(),
        serde_plain_role(layer.role),
    );
    env.insert(
        "EFFIGY_STATE_APPLY_LAYER_MODE".to_owned(),
        serde_plain_apply_mode(layer.apply_mode),
    );
    env.insert(
        "EFFIGY_STATE_APPLY_LAYER_SOURCE".to_owned(),
        layer.source.clone(),
    );
    if let Some(target) = layer.target.as_ref() {
        env.insert("EFFIGY_STATE_APPLY_LAYER_TARGET".to_owned(), target.clone());
    }
    if let Some(hook) = layer.hook.as_ref() {
        env.insert("EFFIGY_STATE_APPLY_HOOK".to_owned(), hook.clone());
    }
    if let Some(artifact_report) = layer.artifact_report.as_ref() {
        if let Some(digest) = artifact_report
            .pointer("/destination/digest")
            .and_then(Value::as_str)
        {
            env.insert("EFFIGY_STATE_APPLY_DIGEST".to_owned(), digest.to_owned());
        }
    }
    env.insert(
        "EFFIGY_STATE_APPLY_CONTEXT".to_owned(),
        context_path.to_owned(),
    );
    env
}

fn write_state_apply_hook_context(
    repo_root: &Path,
    stack_name: &str,
    environment: effigy_state::StateEnvironment,
    lineage_id: &str,
    layer: &StateStackApplyLayerReport,
) -> Result<String, RunnerError> {
    let relative_path = PathBuf::from(".effigy")
        .join("state")
        .join("apply-context")
        .join(safe_path_component(stack_name))
        .join(format!("{}.json", safe_path_component(&layer.key)));
    let absolute_path = repo_root.join(&relative_path);
    let Some(parent) = absolute_path.parent() else {
        return Err(RunnerError::task_invocation(format!(
            "failed to resolve parent directory for {}",
            absolute_path.display()
        )));
    };
    fs::create_dir_all(parent).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create state apply context directory {}: {error}",
            parent.display()
        ))
    })?;
    let context = StateApplyHookContext {
        schema: "effigy.state-stack.apply-context.v1".to_owned(),
        schema_version: 1,
        stack_name: stack_name.to_owned(),
        environment: serde_plain_state_environment(environment),
        lineage_id: lineage_id.to_owned(),
        layer: StateApplyHookLayerContext {
            index: layer.index,
            key: layer.key.clone(),
            role: serde_plain_role(layer.role),
            apply_mode: serde_plain_apply_mode(layer.apply_mode),
            source: layer.source.clone(),
            target: layer.target.clone(),
            hook: layer.hook.clone(),
            status: layer.status.to_string(),
            output: layer.output.clone(),
            artifact_report: layer.artifact_report.clone(),
            sql_report: layer.sql_report.clone(),
        },
    };
    let encoded = serde_json::to_string_pretty(&context)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    fs::write(&absolute_path, format!("{encoded}\n")).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write state apply context {}: {error}",
            path_display(&absolute_path, repo_root)
        ))
    })?;
    Ok(absolute_path.display().to_string())
}

#[derive(Debug, Serialize)]
struct StateCaptureTaskContext {
    schema: String,
    schema_version: u8,
    stack_name: String,
    parent_lineage_id: String,
    capture_role: String,
    capture_mode: String,
    source_environment: String,
    key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    destination_ref: Option<String>,
}

#[derive(Debug, Serialize)]
struct StateApplyHookContext {
    schema: String,
    schema_version: u8,
    stack_name: String,
    environment: String,
    lineage_id: String,
    layer: StateApplyHookLayerContext,
}

#[derive(Debug, Serialize)]
struct StateApplyHookLayerContext {
    index: usize,
    key: String,
    role: String,
    apply_mode: String,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hook: Option<String>,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sql_report: Option<Value>,
}

fn serde_plain_role(role: StateLayerRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{role:?}"))
}

fn serde_plain_apply_mode(mode: StateLayerApplyMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{mode:?}"))
}

fn serde_plain_state_environment(environment: effigy_state::StateEnvironment) -> String {
    serde_json::to_value(environment)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{environment:?}"))
}

fn parse_capture_role(role: &str) -> Result<StateLayerRole, RunnerError> {
    match role {
        "uat-capture" => Ok(StateLayerRole::UatCapture),
        "full-capture" => Ok(StateLayerRole::FullCapture),
        _ => Err(RunnerError::task_invocation(format!(
            "`state capture` role must be `uat-capture` or `full-capture`, got `{role}`"
        ))),
    }
}

#[derive(Debug, Serialize)]
struct StateStackCaptureArtifact {
    layer_key: String,
    operation: StateCaptureArtifactOperation,
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    ref_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_report: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StateCaptureArtifactOperation {
    PlannedCapture,
    CapturedLocal,
    Pushed,
}

#[derive(Debug, Serialize)]
struct StateStackCaptureTask {
    name: String,
    status: StateStackCaptureTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StateStackCaptureTaskStatus {
    Planned,
    Executed,
    Failed,
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
    use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};
    use effigy_state::{
        StateEnvironment, StateLayerApplyMode, StateLayerEnvironmentPolicy, StateLayerRole,
        StateStackLineageLayer, StateStackLineageReport,
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

        let report = StateStackCaptureReport::from_request(
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

        let rendered = fs::read_to_string(
            repo.join(".effigy/reports/state/acowtancy-uat/latest-apply.json"),
        )
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
