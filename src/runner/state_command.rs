use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use effigy_artifacts::OciArtifactAdapter;
use effigy_cli::{BootstrapDbSeedInput, StateArgs, StateSubcommand, TaskInvocation};
use effigy_execution::ExecutionSurface;
use effigy_state::{
    StateLayerApplyMode, StateLayerEnvironmentPolicy, StateLayerRole, StateStackLineageReport,
    StateStackManifest,
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
        } => run_state_apply(
            manifest.as_deref(),
            stack.as_deref(),
            &context.invocation_cwd,
            &context.resolved.resolved_root,
            yes,
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
                task,
                yes,
                push,
            },
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
    output_json: bool,
) -> Result<String, RunnerError> {
    let manifest = resolve_state_stack_manifest(manifest, stack, invocation_cwd, repo_root)?;
    let lineage = manifest
        .plan_lineage()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .report("planned");
    let mut report = StateStackApplyReport::from_lineage(&lineage, execute);

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

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_apply_text(&report))
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

    if output_json {
        return serde_json::to_string(&report)
            .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }

    Ok(render_state_capture_text(&report))
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
    )?;

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
            "multiple state stacks are defined; set `state.default` or pass a stack name: {}",
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
        if let Some(error) = layer.error.as_deref() {
            lines.push(format!("  error: {error}"));
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

fn state_report_write_paths(
    repo_root: &Path,
    stack_name: &str,
    kind: StateHistoryKind,
    lineage: Option<&str>,
    compatibility_file: Option<&str>,
) -> StateReportWritePaths {
    let stack_dir = repo_root
        .join(".effigy")
        .join("reports")
        .join("state")
        .join(safe_path_component(stack_name));
    let latest_path = stack_dir.join(format!("latest-{kind}.json"));
    let history_path = state_history_report_path(
        &stack_dir,
        kind,
        &utc_basic_timestamp(SystemTime::now()),
        &short_safe_lineage(lineage.unwrap_or("lineage-unknown")),
    );
    let compatibility_path = compatibility_file.map(|file| stack_dir.join(file));
    StateReportWritePaths {
        compatibility_path,
        latest_path,
        history_path,
    }
}

fn state_history_report_path(
    stack_dir: &Path,
    kind: StateHistoryKind,
    timestamp: &str,
    lineage: &str,
) -> PathBuf {
    let history_dir = stack_dir.join("history");
    let base_name = format!("{timestamp}-{kind}-{lineage}");
    let mut path = history_dir.join(format!("{base_name}.json"));
    let mut suffix = 2;
    while path.exists() {
        path = history_dir.join(format!("{base_name}-{suffix}.json"));
        suffix += 1;
    }
    path
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

#[derive(Debug)]
struct StateReportWritePaths {
    compatibility_path: Option<PathBuf>,
    latest_path: PathBuf,
    history_path: PathBuf,
}

impl StateReportWritePaths {
    fn all_paths(&self) -> impl Iterator<Item = &Path> {
        self.compatibility_path
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(self.latest_path.as_path()))
            .chain(std::iter::once(self.history_path.as_path()))
    }
}

fn short_safe_lineage(lineage: &str) -> String {
    let safe = safe_path_component(lineage);
    safe.chars().take(48).collect()
}

fn utc_basic_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_unix_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

fn civil_from_unix_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
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
    layers: Vec<effigy_state::StateStackLayer>,
    #[serde(default)]
    captures: BTreeMap<String, ManifestStateCaptureProfile>,
}

impl ManifestStateStackConfig {
    fn into_manifest(self) -> StateStackManifest {
        StateStackManifest {
            schema: self.schema,
            name: self.name,
            environment: self.environment,
            layers: self.layers,
        }
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
    task: Option<String>,
    #[serde(default)]
    push: bool,
}

#[derive(Debug, Serialize)]
struct StateStackApplyReport {
    schema: String,
    schema_version: u8,
    ok: bool,
    executed: bool,
    stack_name: String,
    environment: effigy_state::StateEnvironment,
    lineage_id: String,
    layers: Vec<StateStackApplyLayerReport>,
    warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    written_history_path: Option<String>,
}

impl StateStackApplyReport {
    fn from_lineage(lineage: &StateStackLineageReport, execute: bool) -> Self {
        let layers = lineage
            .layers
            .iter()
            .map(|layer| {
                let status = match layer.apply_mode {
                    StateLayerApplyMode::Task if execute => StateStackApplyLayerStatus::PlannedTask,
                    StateLayerApplyMode::Artifact if execute => {
                        StateStackApplyLayerStatus::PlannedArtifactStage
                    }
                    StateLayerApplyMode::Sql if execute => {
                        StateStackApplyLayerStatus::PlannedSqlImport
                    }
                    StateLayerApplyMode::Task => StateStackApplyLayerStatus::WouldExecute,
                    StateLayerApplyMode::Artifact => StateStackApplyLayerStatus::WouldStage,
                    StateLayerApplyMode::Sql => StateStackApplyLayerStatus::WouldImport,
                    _ => StateStackApplyLayerStatus::Unsupported,
                };
                StateStackApplyLayerReport {
                    index: layer.index,
                    key: layer.key.clone(),
                    role: layer.role,
                    apply_mode: layer.apply_mode,
                    source: layer.source.clone(),
                    target: layer.sql_target.clone(),
                    status,
                    output: None,
                    artifact_report: None,
                    sql_report: None,
                    error: None,
                }
            })
            .collect();
        Self {
            schema: "effigy.state-stack.apply.v1".to_owned(),
            schema_version: 1,
            ok: true,
            executed: execute,
            stack_name: lineage.stack_name.clone(),
            environment: lineage.environment,
            lineage_id: lineage.lineage_id.clone(),
            layers,
            warnings: lineage.warnings.clone(),
            written_report_path: None,
            written_history_path: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct StateStackApplyLayerReport {
    index: usize,
    key: String,
    role: effigy_state::StateLayerRole,
    apply_mode: StateLayerApplyMode,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    status: StateStackApplyLayerStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sql_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StateStackApplyLayerStatus {
    WouldExecute,
    WouldStage,
    WouldImport,
    PlannedTask,
    PlannedArtifactStage,
    PlannedSqlImport,
    Executed,
    Staged,
    Imported,
    Unsupported,
    Failed,
}

impl std::fmt::Display for StateStackApplyLayerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::WouldExecute => "would-execute",
            Self::WouldStage => "would-stage",
            Self::WouldImport => "would-import",
            Self::PlannedTask => "planned-task",
            Self::PlannedArtifactStage => "planned-artifact-stage",
            Self::PlannedSqlImport => "planned-sql-import",
            Self::Executed => "executed",
            Self::Staged => "staged",
            Self::Imported => "imported",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Serialize)]
struct StateStackHistoryReport {
    schema: String,
    schema_version: u8,
    stack_name: String,
    reports: Vec<StateStackHistoryItem>,
    warnings: Vec<String>,
}

impl StateStackHistoryReport {
    fn scan(
        repo_root: &Path,
        stack: &str,
        kind: Option<StateHistoryKind>,
        limit: usize,
        lineage: Option<&str>,
    ) -> Result<Self, RunnerError> {
        let mut warnings = Vec::new();
        let stack_dir = repo_root
            .join(".effigy")
            .join("reports")
            .join("state")
            .join(safe_path_component(stack));
        let mut candidates = Vec::new();
        collect_state_history_candidates(&stack_dir, &mut candidates, &mut warnings);
        collect_state_history_candidates(
            &stack_dir.join("history"),
            &mut candidates,
            &mut warnings,
        );

        let mut reports = Vec::new();
        for path in candidates {
            match read_state_history_item(repo_root, &path) {
                Ok(Some(item)) => {
                    if kind.is_some_and(|expected| item.kind != expected) {
                        continue;
                    }
                    if let Some(lineage) = lineage {
                        let matches_lineage = item.lineage_id.as_deref() == Some(lineage)
                            || item.parent_lineage_id.as_deref() == Some(lineage);
                        if !matches_lineage {
                            continue;
                        }
                    }
                    reports.push(item);
                }
                Ok(None) => {}
                Err(error) => warnings.push(error),
            }
        }
        reports.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.path.cmp(&left.path))
        });
        reports.truncate(limit);

        Ok(Self {
            schema: "effigy.state-stack.history.v1".to_owned(),
            schema_version: 1,
            stack_name: stack.to_owned(),
            reports,
            warnings,
        })
    }
}

fn collect_state_history_candidates(
    dir: &Path,
    candidates: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension == "json")
                {
                    candidates.push(path);
                }
            }
            Err(error) => warnings.push(format!(
                "failed to read state history entry in {}: {error}",
                dir.display()
            )),
        }
    }
}

fn read_state_history_item(
    repo_root: &Path,
    path: &Path,
) -> Result<Option<StateStackHistoryItem>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read state report {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("ignored malformed state report {}: {error}", path.display()))?;
    let schema = value
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(kind) =
        StateHistoryKind::from_schema(&schema).or_else(|| StateHistoryKind::from_path(path))
    else {
        return Ok(None);
    };
    let lineage_id = value
        .get("lineage_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let parent_lineage_id = value
        .get("parent_lineage_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let created_at = value
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| path_created_at_fallback(path));
    let ok = value.get("ok").and_then(Value::as_bool);
    let executed = value.get("executed").and_then(Value::as_bool);
    let path_display = path_display(path, repo_root);
    Ok(Some(StateStackHistoryItem {
        kind,
        schema,
        path: path_display,
        created_at,
        lineage_id,
        parent_lineage_id,
        ok,
        executed,
        summary: state_history_summary(kind, &value),
    }))
}

fn path_created_at_fallback(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

fn state_history_summary(kind: StateHistoryKind, value: &Value) -> String {
    match kind {
        StateHistoryKind::Plan => value
            .get("layers")
            .and_then(Value::as_array)
            .map(|layers| format!("{} planned layer(s)", layers.len()))
            .unwrap_or_else(|| "plan report".to_owned()),
        StateHistoryKind::Apply => value
            .get("layers")
            .and_then(Value::as_array)
            .map(|layers| format!("{} apply layer(s)", layers.len()))
            .unwrap_or_else(|| "apply report".to_owned()),
        StateHistoryKind::Capture => value
            .get("produced_layers")
            .and_then(Value::as_array)
            .map(|layers| format!("{} produced layer(s)", layers.len()))
            .unwrap_or_else(|| "capture report".to_owned()),
    }
}

fn parse_state_history_kind(kind: &str) -> Result<StateHistoryKind, RunnerError> {
    StateHistoryKind::parse(kind).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "`state history --kind` must be `plan`, `apply`, or `capture`, got `{kind}`"
        ))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StateHistoryKind {
    Plan,
    Apply,
    Capture,
}

impl StateHistoryKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "plan" => Some(Self::Plan),
            "apply" => Some(Self::Apply),
            "capture" => Some(Self::Capture),
            _ => None,
        }
    }

    fn from_schema(schema: &str) -> Option<Self> {
        match schema {
            effigy_state::STATE_STACK_LINEAGE_SCHEMA => Some(Self::Plan),
            "effigy.state-stack.apply.v1" => Some(Self::Apply),
            "effigy.state-stack.capture.v1" => Some(Self::Capture),
            _ => None,
        }
    }

    fn from_path(path: &Path) -> Option<Self> {
        let file_name = path.file_name()?.to_str()?;
        if file_name.contains("plan") {
            Some(Self::Plan)
        } else if file_name.contains("apply") {
            Some(Self::Apply)
        } else if file_name.contains("capture") {
            Some(Self::Capture)
        } else {
            None
        }
    }
}

impl std::fmt::Display for StateHistoryKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Plan => "plan",
            Self::Apply => "apply",
            Self::Capture => "capture",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Serialize)]
struct StateStackHistoryItem {
    kind: StateHistoryKind,
    schema: String,
    path: String,
    created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lineage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_lineage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    executed: Option<bool>,
    summary: String,
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
    task: Option<String>,
    yes: bool,
    push: bool,
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
        let capture_mode = StateCaptureMode::from_role(capture_role);
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
        let produced_layer = StateStackCaptureProducedLayer {
            key: key.clone(),
            role: capture_role,
            apply_mode: StateLayerApplyMode::Artifact,
            environment_policy: capture_mode.environment_policy(),
            artifact_kind: Some(effigy_artifacts::ArtifactKind::AppSpecific),
            source_ref: request.destination_ref.clone(),
            snapshot_identity: Some(format!(
                "{}@planned",
                capture_mode.snapshot_identity_prefix()
            )),
            depends_on: lineage
                .layers
                .last()
                .map(|layer| vec![layer.key.clone()])
                .unwrap_or_default(),
            hook: request.hook.clone(),
        };
        let mut warnings = lineage.warnings.clone();
        if request.destination_ref.is_none() {
            warnings.push(
                "capture destination ref is not specified; produced layer source is unresolved"
                    .to_owned(),
            );
        }
        let mut tasks = request
            .task
            .clone()
            .into_iter()
            .map(|name| StateStackCaptureTask {
                name,
                status: StateStackCaptureTaskStatus::Planned,
                context_path: None,
                output: None,
                error: None,
            })
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
                match crate::runner::execute::api::run_manifest_task_with_surface_and_env(
                    &TaskInvocation {
                        name: task.name.clone(),
                        args: Vec::new(),
                    },
                    repo_root.to_path_buf(),
                    ExecutionSurface::DirectCli,
                    &state_capture_task_env(
                        lineage,
                        &request,
                        capture_role,
                        capture_mode,
                        context_path.as_deref(),
                    ),
                ) {
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
        env.insert("EFFIGY_STATE_CAPTURE_SOURCE".to_owned(), source.clone());
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

fn serde_plain_role(role: StateLayerRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{role:?}"))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StateCaptureMode {
    UatOverlay,
    FullSnapshot,
}

impl StateCaptureMode {
    fn from_role(role: StateLayerRole) -> Self {
        match role {
            StateLayerRole::UatCapture => Self::UatOverlay,
            StateLayerRole::FullCapture => Self::FullSnapshot,
            _ => unreachable!("capture roles are validated before mode selection"),
        }
    }

    fn environment_policy(self) -> StateLayerEnvironmentPolicy {
        match self {
            Self::UatOverlay => StateLayerEnvironmentPolicy::NonProduction,
            Self::FullSnapshot => StateLayerEnvironmentPolicy::CaptureOnly,
        }
    }

    fn snapshot_identity_prefix(self) -> &'static str {
        match self {
            Self::UatOverlay => "uat-authored-content",
            Self::FullSnapshot => "full-system-capture",
        }
    }
}

impl std::fmt::Display for StateCaptureMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::UatOverlay => "uat-overlay",
            Self::FullSnapshot => "full-snapshot",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Serialize)]
struct StateStackCaptureProducedLayer {
    key: String,
    role: StateLayerRole,
    apply_mode: StateLayerApplyMode,
    environment_policy: StateLayerEnvironmentPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_kind: Option<effigy_artifacts::ArtifactKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshot_identity: Option<String>,
    depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hook: Option<String>,
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use effigy_artifacts::{
        OciArtifactDescriptor, OciArtifactError, OciArtifactInspectRequest, OciArtifactPullReport,
        OciArtifactPullRequest, OciArtifactPushReport, OciArtifactPushRequest,
    };
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
